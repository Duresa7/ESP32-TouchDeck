use serde_json::{Value, json};
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use td_companion::{
    api::NoQuery,
    features::{
        assets::{AssetPath, CatalogQuery, CatalogSearch, Create, CreateAsset, File},
        icon_packs::{Install, List, PackIconPath, PackPath, Remove, Thumbnail},
    },
    jobs::JobState,
    testing::TestCompanion,
};
use td_host::{
    catalog::{Error, Fetcher},
    packs::{CUSTOM_LICENSE, Curated, PackAuthor, PackLicense, PackOrigin, REGISTRY, integrity_of},
};

const MONO: &str = r##"{
  "prefix": "mono",
  "width": 24,
  "height": 24,
  "info": {"name": "Mono"},
  "icons": {
    "spotify": {"body": "<circle cx=\"12\" cy=\"12\" r=\"10\" fill=\"currentColor\"/>"},
    "bar": {"body": "<path fill=\"#ff0000\" d=\"M0 0h12v24H0z\"/>"}
  },
  "aliases": {
    "music": {"parent": "spotify"},
    "bar-right": {"parent": "bar", "hFlip": true}
  }
}"##;

/// A gzipped ustar archive, as npm publishes a package.
fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar = Vec::new();
    for (name, data) in files {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..108].copy_from_slice(b"0000644\0");
        header[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        tar.extend_from_slice(&header);
        tar.extend_from_slice(data);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.extend_from_slice(&[0; 1024]);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar).unwrap();
    gz.finish().unwrap()
}

fn listed(tarball: &[u8]) -> Curated {
    Curated {
        prefix: "mono".into(),
        name: "Mono".into(),
        package: "@iconify-json/mono".into(),
        version: "1.0.0".into(),
        tarball: format!("{REGISTRY}@iconify-json/mono/-/mono-1.0.0.tgz"),
        integrity: integrity_of(tarball),
        unpacked_size: 4096,
        icons: 3,
        license: PackLicense {
            title: "CC BY 4.0".into(),
            spdx: Some("CC-BY-4.0".into()),
            url: Some("https://creativecommons.org/licenses/by/4.0/".into()),
        },
        author: PackAuthor {
            name: "Someone".into(),
            url: Some("https://example.org".into()),
        },
        attribution: true,
        brands: true,
        palette: false,
        samples: vec!["spotify".into()],
    }
}

/// Serves one tarball for the registry URL and counts the downloads.
struct Registry(Vec<u8>, Arc<AtomicUsize>);
impl Fetcher for Registry {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        assert_eq!(
            url,
            format!("{REGISTRY}@iconify-json/mono/-/mono-1.0.0.tgz")
        );
        self.1.fetch_add(1, Ordering::SeqCst);
        Ok(self.0.clone())
    }
}

async fn start(served: Vec<u8>, pinned: &[u8]) -> (TestCompanion, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let td = TestCompanion::start_with_packs(
        vec![listed(pinned)],
        Box::new(Registry(served, calls.clone())),
    )
    .await;
    (td, calls)
}

async fn search(td: &TestCompanion, q: &str) -> Vec<td_companion::features::assets::CatalogHit> {
    td.call::<CatalogSearch>(
        (),
        CatalogQuery {
            q: q.into(),
            catalog: None,
        },
        (),
    )
    .await
    .unwrap()
}

async fn render(td: &TestCompanion, id: &str) -> Vec<u8> {
    let body: CreateAsset = serde_json::from_value(
        json!({"source": {"kind": "catalog", "catalog": "iconify:mono", "id": id}}),
    )
    .unwrap();
    let job = td.call::<Create>((), NoQuery, body).await.unwrap();
    let done = td.wait(&job).await;
    assert_eq!(done.state, JobState::Succeeded, "{done:?}");
    let asset = done.result.unwrap()["id"].as_str().unwrap().to_owned();
    td.request::<File>(AssetPath { asset }, NoQuery, &())
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec()
}

/// The RGB565 colour and alpha of a TDI1 pixel.
fn pixel(file: &[u8], x: usize, y: usize) -> (u16, u8) {
    let at = y * 64 + x;
    let header = td_host::icon_pack::HEADER_BYTES;
    let color = u16::from_le_bytes([file[header + at * 2], file[header + at * 2 + 1]]);
    (color, file[header + td_host::icon_pack::COLOR_BYTES + at])
}

fn thumbnail(td: &TestCompanion, icon: &str) -> reqwest::RequestBuilder {
    td.request::<Thumbnail>(
        PackIconPath {
            prefix: "mono".into(),
            icon: icon.into(),
        },
        NoQuery,
        &(),
    )
}

async fn failure(request: reqwest::RequestBuilder) -> (u16, Value) {
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (
        status,
        response.json::<Value>().await.unwrap()["error"].clone(),
    )
}

#[tokio::test]
async fn install_search_render_thumbnail_and_remove() {
    let gz = tarball(&[
        ("package/icons.json", MONO.as_bytes()),
        ("package/info.json", br#"{"prefix":"mono"}"#),
        ("package/LICENSE", b"CC BY 4.0"),
        ("package/index.js", b"module.exports = {}"),
    ]);
    let (td, calls) = start(gz.clone(), &gz).await;
    let list = td.call::<List>((), NoQuery, ()).await.unwrap();
    assert_eq!(list.len(), 1);
    assert!(!list[0].installed);
    assert!(
        search(&td, "spotify")
            .await
            .iter()
            .all(|h| h.source != "iconify:mono")
    );

    let job = td
        .call::<Install>(
            PackPath {
                prefix: "mono".into(),
            },
            NoQuery,
            (),
        )
        .await
        .unwrap();
    assert_eq!(job.kind, "icon_packs.install");
    let done = td.wait(&job).await;
    assert_eq!(done.state, JobState::Succeeded, "{done:?}");
    assert_eq!(done.progress["stage"], "done");
    let result = done.result.unwrap();
    assert_eq!(
        (result["installed"].as_bool(), result["icons"].as_u64()),
        (Some(true), Some(3))
    );
    assert!(result["size_bytes"].as_u64().unwrap() > 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let list = td.call::<List>((), NoQuery, ()).await.unwrap();
    assert!(list[0].installed && list[0].origin == PackOrigin::Curated);

    // Pack hits join the catalog's ranking: an exact pack name outranks a catalog substring.
    let hits = search(&td, "spotify").await;
    let first = hits
        .iter()
        .position(|h| h.source == "iconify:mono")
        .unwrap();
    assert!(
        hits[..first]
            .iter()
            .all(|h| h.name.eq_ignore_ascii_case("spotify"))
    );
    let hit = &hits[first];
    assert_eq!(
        (
            hit.id.as_str(),
            hit.license.as_str(),
            hit.category.as_deref()
        ),
        ("spotify", "CC-BY-4.0", Some("Mono"))
    );
    assert_eq!(hit.aliases, ["music"]);
    assert!(
        hit.attribution
            .contains("Icons from Mono by Someone (CC BY 4.0)")
    );
    assert!(hit.attribution.contains("Logos belong to their owners"));
    let only = td
        .call::<CatalogSearch>(
            (),
            CatalogQuery {
                q: "music".into(),
                catalog: Some("iconify:mono".into()),
            },
            (),
        )
        .await
        .unwrap();
    assert_eq!(
        only.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
        ["spotify"]
    );
    assert!(search(&td, "a").await.len() <= td_companion::features::icon_packs::SEARCH_LIMIT);

    // currentColor draws in the built-in ink, so a monochrome icon shows on a dark Tile.
    let file = render(&td, "spotify").await;
    assert!(td_host::icon_pack::is_icon(&file));
    let ink = (0xf2u16 >> 3) << 11 | (0xf5u16 >> 2) << 5 | (0xfau16 >> 3);
    assert_eq!(pixel(&file, 32, 32), (ink, 255));
    assert_eq!(pixel(&file, 0, 0).1, 0);
    // The flipped alias draws the red bar on the right.
    let flipped = render(&td, "bar-right").await;
    assert_eq!(pixel(&flipped, 48, 32), (0xf800, 255));
    assert_eq!(pixel(&flipped, 16, 32).1, 0);

    let response = thumbnail(&td, "spotify.svg").send().await.unwrap();
    assert_eq!(response.status(), 200);
    let headers = response.headers().clone();
    assert_eq!(headers["content-type"], "image/svg+xml");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    let port = td.url.rsplit(':').next().unwrap();
    assert_eq!(
        headers["content-security-policy"],
        format!(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' blob: data:; connect-src 'self' ws://127.0.0.1:{port}; \
         font-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; \
         frame-ancestors 'none'"
        )
    );
    let svg = response.text().await.unwrap();
    assert!(
        svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
        "{svg}"
    );
    assert!(svg.contains("color=\"#f2f5fa\"") && svg.contains("<circle"));
    assert!(
        thumbnail(&td, "bar-right.svg")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
            .contains("scale(-1 1)")
    );
    for missing in ["nope.svg", "spotify", "spotify.png"] {
        let (status, error) = failure(thumbnail(&td, missing)).await;
        assert_eq!(
            (status, error["code"].as_str()),
            (404, Some("icon_pack.not_found")),
            "{missing}"
        );
    }

    // Installing again is a no-op that downloads nothing.
    let again = td
        .call::<Install>(
            PackPath {
                prefix: "mono".into(),
            },
            NoQuery,
            (),
        )
        .await
        .unwrap();
    assert_eq!(td.wait(&again).await.state, JobState::Succeeded);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    td.call::<Remove>(
        PackPath {
            prefix: "mono".into(),
        },
        NoQuery,
        (),
    )
    .await
    .unwrap();
    assert!(!td.call::<List>((), NoQuery, ()).await.unwrap()[0].installed);
    assert!(
        search(&td, "spotify")
            .await
            .iter()
            .all(|h| h.source != "iconify:mono")
    );
    assert_eq!(failure(thumbnail(&td, "spotify.svg")).await.0, 404);
    let (status, error) = failure(td.request::<Remove>(
        PackPath {
            prefix: "mono".into(),
        },
        NoQuery,
        &(),
    ))
    .await;
    assert_eq!(
        (status, error["code"].as_str()),
        (404, Some("icon_pack.not_found"))
    );
}

#[tokio::test]
async fn a_download_that_fails_its_pinned_integrity_is_refused() {
    let pinned = tarball(&[("package/icons.json", MONO.as_bytes())]);
    let tampered = tarball(&[(
        "package/icons.json",
        MONO.replace("#ff0000", "#00ff00").as_bytes(),
    )]);
    let (td, calls) = start(tampered, &pinned).await;
    let job = td
        .call::<Install>(
            PackPath {
                prefix: "mono".into(),
            },
            NoQuery,
            (),
        )
        .await
        .unwrap();
    let done = td.wait(&job).await;
    assert_eq!(done.state, JobState::Failed);
    assert_eq!(done.error.unwrap().code, "icon_pack.integrity");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!td.call::<List>((), NoQuery, ()).await.unwrap()[0].installed);
    assert!(
        search(&td, "spotify")
            .await
            .iter()
            .all(|h| h.source != "iconify:mono")
    );
    let (status, error) = failure(td.request::<Install>(
        PackPath {
            prefix: "other".into(),
        },
        NoQuery,
        &(),
    ))
    .await;
    assert_eq!(
        (status, error["code"].as_str()),
        (404, Some("icon_pack.not_found"))
    );
}

#[tokio::test]
async fn imports_are_validated_marked_custom_and_refuse_clashes() {
    let gz = tarball(&[("package/icons.json", MONO.as_bytes())]);
    let (td, _) = start(gz.clone(), &gz).await;
    let import = |body: Vec<u8>, kind: &str| {
        td.raw()
            .post("/api/v1/icon-packs/import")
            .bearer_auth(&td.token)
            .header("content-type", kind)
            .body(body)
    };
    let mine = MONO.replace("\"mono\"", "\"mine\"");
    let (status, _) = failure(import(mine.clone().into_bytes(), "text/plain")).await;
    assert_eq!(status, 415);
    for bad in [
        b"not json".to_vec(),
        br#"{"prefix":"Bad Prefix","icons":{"a":{"body":"<path/>"}}}"#.to_vec(),
        br#"{"prefix":"empty","icons":{}}"#.to_vec(),
        br#"{"prefix":"evil","icons":{"a":{"body":"<image href=\"https://example.org/x.png\"/>"}}}"#.to_vec(),
    ] {
        let (status, error) = failure(import(bad, "application/json")).await;
        assert_eq!((status, error["code"].as_str()), (422, Some("icon_pack.invalid")));
    }
    let response = import(mine.clone().into_bytes(), "application/json")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let info: Value = response.json().await.unwrap();
    assert_eq!(info["origin"], "imported");
    assert_eq!(info["license"]["title"], CUSTOM_LICENSE);
    assert_eq!(
        (info["name"].as_str(), info["icons"].as_u64()),
        (Some("Mono"), Some(3))
    );
    assert!(
        search(&td, "spotify")
            .await
            .iter()
            .any(|h| h.source == "iconify:mine" && h.license == CUSTOM_LICENSE)
    );
    for clash in [mine.into_bytes(), MONO.as_bytes().to_vec()] {
        let (status, error) = failure(import(clash, "application/json")).await;
        assert_eq!(
            (status, error["code"].as_str()),
            (409, Some("icon_pack.exists"))
        );
    }
    let list = td.call::<List>((), NoQuery, ()).await.unwrap();
    assert_eq!(
        list.iter()
            .map(|p| (p.prefix.as_str(), p.installed))
            .collect::<Vec<_>>(),
        [("mono", false), ("mine", true)]
    );
    let (status, _) = failure(import(vec![b' '; (32 << 20) + 1], "application/json")).await;
    assert_eq!(status, 413);
}

#[tokio::test]
async fn svg_uploads_render_without_reading_outside_files() {
    let td = TestCompanion::start().await;
    let dir = tempfile::tempdir().unwrap();
    let secret = dir.path().join("secret.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([0, 255, 0, 255]))
        .save(&secret)
        .unwrap();
    let href = secret.to_string_lossy().replace('\\', "/");
    let svg = format!(
        r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 20 20">
  <rect width="10" height="20" fill="currentColor"/>
  <image x="10" width="10" height="20" href="{href}"/>
  <image x="10" width="10" height="20" xlink:href="file:///{href}"/>
</svg>"#
    );
    let upload = |body: Vec<u8>| {
        td.raw()
            .post("/api/v1/uploads")
            .bearer_auth(&td.token)
            .header("content-type", "image/svg+xml")
            .body(body)
    };
    let uploaded: Value = upload(svg.into_bytes())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(uploaded["media_type"], "image/svg+xml");
    let body: CreateAsset =
        serde_json::from_value(json!({"source": {"kind": "upload", "upload": uploaded["id"]}}))
            .unwrap();
    let job = td.call::<Create>((), NoQuery, body).await.unwrap();
    let done = td.wait(&job).await;
    assert_eq!(done.state, JobState::Succeeded, "{done:?}");
    let asset = done.result.unwrap()["id"].as_str().unwrap().to_owned();
    let file = td
        .request::<File>(AssetPath { asset }, NoQuery, &())
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let ink = (0xf2u16 >> 3) << 11 | (0xf5u16 >> 2) << 5 | (0xfau16 >> 3);
    assert_eq!(pixel(&file, 20, 32), (ink, 255));
    // The linked file was never read: that half stays transparent.
    assert_eq!(pixel(&file, 44, 32).1, 0);

    let (status, error) = failure(upload(
        b"<svg xmlns='http://www.w3.org/2000/svg'><rect".to_vec(),
    ))
    .await;
    assert_eq!(
        (status, error["code"].as_str()),
        (400, Some("request.invalid"))
    );
    let (status, _) = failure(upload(vec![b'<'; (16 << 20) + 1])).await;
    assert_eq!(status, 413);
}
