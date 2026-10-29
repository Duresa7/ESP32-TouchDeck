//! Assets: the built-in icons, icons taken from programs, Asset files, and removing the Assets
//! no stored Layout uses. Uploads and the Device's Asset list come with #93.
use crate::{
    api::{ApiError, CommonError, Ctx, Endpoint, Feature, JobKind, NoQuery, Registration, Req},
    features::devices::DevicePath,
    jobs::{Job, JobError, JobHandle, JobScope, Stop},
    link::DeviceLinkError,
};
use axum::{
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use td_core::{Layout, Orientation, layout::Action};
use td_host::{catalog, icon_pack, icons};
use td_link::xfer::{self, XferError};
use ts_rs::TS;

/// A built-in icon and the id of its Asset file.
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct Builtin {
    pub name: String,
    pub label: String,
    pub tags: Vec<String>,
    pub id: String,
}
impl From<&icons::Builtin> for Builtin {
    fn from(icon: &icons::Builtin) -> Self {
        Self {
            name: icon.name.clone(),
            label: icon.label.clone(),
            tags: icon.tags.clone(),
            id: icon.id.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("The Asset does not exist.")]
    NotFound,
    #[error("The image or icon couldn't be rendered: {0}")]
    RenderFailed(String),
    #[error(transparent)]
    Device(#[from] DeviceLinkError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error(transparent)]
    Common(#[from] CommonError),
}
impl ApiError for AssetError {
    const CODES: &'static [&'static str] = &[
        "asset.not_found",
        "asset.render_failed",
        "device.not_found",
        "device.not_connected",
        "device.timeout",
        "device.link",
        "device.busy",
    ];
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::RenderFailed(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Device(e) => e.status(),
            Self::Job(e) => e.status(),
            Self::Common(e) => e.status(),
        }
    }
    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "asset.not_found",
            Self::RenderFailed(_) => "asset.render_failed",
            Self::Device(e) => e.code(),
            Self::Job(e) => e.code(),
            Self::Common(e) => e.code(),
        }
    }
    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::Device(e) => e.details(),
            Self::Job(e) => e.details(),
            Self::Common(e) => e.details(),
            _ => None,
        }
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, CommonError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| CommonError::Internal)
}

pub struct BuiltinList;
impl Endpoint for BuiltinList {
    const NAME: &'static str = "assets.builtin";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/assets/builtin";
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<Builtin>;
    type Error = CommonError;
    async fn handle(_: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        // The set renders once, on first use.
        blocking(|| icons::builtin().iter().map(Builtin::from).collect()).await
    }
}

#[derive(Serialize, Deserialize, TS)]
pub struct SuggestRequest {
    pub action: Action,
}
/// The built-in icon a Tile's action suggests, if any.
pub struct Suggest;
impl Endpoint for Suggest {
    const NAME: &'static str = "assets.suggest";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/assets/builtin/suggest";
    type Path = ();
    type Query = NoQuery;
    type Body = SuggestRequest;
    type Ok = Option<Builtin>;
    type Error = CommonError;
    async fn handle(_: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let action = serde_json::to_value(&req.body.action).map_err(|_| CommonError::Internal)?;
        blocking(move || {
            let name = icons::default_for(&action)?;
            icons::builtin()
                .iter()
                .find(|icon| icon.name == name)
                .map(Builtin::from)
        })
        .await
    }
}

#[derive(Serialize, Deserialize, TS)]
pub struct AssetPath {
    pub asset: String,
}
/// An Asset file's bytes. The response body is the raw file with its media type.
#[derive(Serialize, Deserialize, TS)]
#[ts(type = "Blob")]
pub struct AssetBytes {
    pub media_type: String,
    pub bytes: Vec<u8>,
}
pub const TDI1: &str = "application/vnd.touchdeck.tdi1";
fn media_type(file: &[u8]) -> &'static str {
    if icon_pack::is_icon(file) {
        TDI1
    } else if file.starts_with(b"GIF87a") || file.starts_with(b"GIF89a") {
        "image/gif"
    } else if file.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else {
        "application/octet-stream"
    }
}
pub struct File;
impl Endpoint for File {
    const NAME: &'static str = "assets.file";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/assets/{asset}";
    type Path = AssetPath;
    type Query = NoQuery;
    type Body = ();
    type Ok = AssetBytes;
    type Error = AssetError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let id = req.path.asset;
        let bytes = blocking(move || ctx.assets.get(&id))
            .await?
            .ok_or(AssetError::NotFound)?;
        Ok(AssetBytes {
            media_type: media_type(&bytes).into(),
            bytes,
        })
    }
    fn respond(value: AssetBytes) -> Response {
        ([(header::CONTENT_TYPE, value.media_type)], value.bytes).into_response()
    }
}

#[derive(Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AssetSource {
    /// The icon of the program at `path`. Only its icon resource is read.
    App { path: String },
    Catalog {
        catalog: String,
        id: String,
        #[serde(default)]
        variant: CatalogVariant,
        #[serde(default)]
        padding: u32,
        background: Option<[u8; 4]>,
        tint: Option<[u8; 3]>,
    },
}
/// How an image that isn't square fills the Tile: whole inside it, or cropped to fill it.
#[derive(Clone, Copy, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    #[default]
    Contain,
    Cover,
}
#[derive(Serialize, Deserialize, TS)]
pub struct CreateAsset {
    pub source: AssetSource,
    #[serde(default)]
    #[ts(optional)]
    pub fit: Option<Fit>,
}
#[derive(Serialize, Deserialize, TS)]
pub struct RenderProgress {
    pub frames_done: u32,
    pub frames_total: u32,
}
#[derive(Serialize, Deserialize, TS)]
pub struct AssetRef {
    pub id: String,
    /// `tdi1`, the Device's icon format.
    pub kind: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}
pub struct RenderJob;
impl JobKind for RenderJob {
    const NAME: &'static str = "assets.render";
    const COMPANION_LIMIT: usize = 2;
    type Progress = RenderProgress;
    type Result = AssetRef;
}
/// Prepares an Asset as a Job. At most two run at once across the Companion.
pub struct Create;
impl Endpoint for Create {
    const NAME: &'static str = "assets.create";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/assets";
    const STATUS: StatusCode = StatusCode::ACCEPTED;
    type Path = ();
    type Query = NoQuery;
    type Body = CreateAsset;
    type Ok = Job;
    type Error = AssetError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let CreateAsset { source, fit } = req.body;
        let work = ctx.clone();
        Ok(ctx
            .jobs
            .start::<RenderJob>(JobScope::Companion, true, move |job| {
                render(&work, source, fit.unwrap_or_default(), job)
            })?)
    }
}
fn render(
    ctx: &Ctx,
    source: AssetSource,
    fit: Fit,
    job: &JobHandle<RenderJob>,
) -> Result<AssetRef, Stop> {
    let progress = |frames_done| RenderProgress {
        frames_done,
        frames_total: 1,
    };
    job.progress(&progress(0));
    let image = match source {
        AssetSource::App { path } => {
            icons::app_icon(std::path::Path::new(&path)).map_err(AssetError::RenderFailed)?
        }
        AssetSource::Catalog {
            catalog,
            id,
            variant,
            padding,
            background,
            tint,
        } => ctx
            .catalog
            .render(
                &catalog,
                &id,
                match variant {
                    CatalogVariant::Base => catalog::Variant::Base,
                    CatalogVariant::Light => catalog::Variant::Light,
                    CatalogVariant::Dark => catalog::Variant::Dark,
                },
                catalog::Options {
                    cover: matches!(fit, Fit::Cover),
                    padding,
                    background,
                    tint,
                    ..Default::default()
                },
            )
            .map_err(|e| AssetError::RenderFailed(e.to_string()))?,
    };
    job.check()?;
    let image = match fit {
        Fit::Contain => image,
        Fit::Cover => {
            let (w, h) = image.dimensions();
            let side = w.min(h);
            image::imageops::crop_imm(&image, (w - side) / 2, (h - side) / 2, side, side).to_image()
        }
    };
    let file = icon_pack::pack_any(&image);
    job.check()?;
    let id = ctx.assets.put(&file).map_err(|_| CommonError::Internal)?;
    job.progress(&progress(1));
    Ok(AssetRef {
        id,
        kind: "tdi1".into(),
        width: icon_pack::SIDE,
        height: icon_pack::SIDE,
        bytes: file.len() as u64,
    })
}

#[derive(Default, Serialize, Deserialize, TS)]
pub struct PruneRequest {
    /// Assets to keep besides the ones the Device's stored Layouts use, such as those of a
    /// Layout being edited.
    #[serde(default)]
    pub also_keep: Vec<String>,
}
#[derive(Serialize, Deserialize, TS)]
pub struct PruneProgress {
    pub removed: u32,
    pub total: u32,
}
#[derive(Serialize, Deserialize, TS)]
pub struct PruneResult {
    pub removed: Vec<String>,
}
pub struct PruneJob;
impl JobKind for PruneJob {
    const NAME: &'static str = "assets.prune";
    type Progress = PruneProgress;
    type Result = PruneResult;
}
/// Removes from the Device every Asset that no stored Layout uses and `also_keep` doesn't name.
pub struct Prune;
impl Endpoint for Prune {
    const NAME: &'static str = "assets.prune";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/devices/{device}/assets/prune";
    const STATUS: StatusCode = StatusCode::ACCEPTED;
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = PruneRequest;
    type Ok = Job;
    type Error = AssetError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let device = req.path.device.as_str().to_owned();
        ctx.links.connected(&device)?;
        let keep: BTreeSet<String> = req.body.also_keep.into_iter().collect();
        let scope = JobScope::Device(device.clone());
        let work = ctx.clone();
        Ok(ctx
            .jobs
            .start::<PruneJob>(scope, true, move |job| prune(&work, &device, keep, job))?)
    }
}
fn prune(
    ctx: &Ctx,
    device: &str,
    mut keep: BTreeSet<String>,
    job: &JobHandle<PruneJob>,
) -> Result<PruneResult, Stop> {
    let link = |e: XferError| Stop::from(DeviceLinkError::from_xfer(e));
    for layout in ctx.layouts.all(device) {
        keep.extend(icons::file_ids(&layout));
    }
    ctx.links.with(device, |session| {
        for orientation in [Orientation::Landscape, Orientation::Portrait] {
            // A slot with no stored Layout uses no Asset.
            if let Ok(report) = xfer::get_layout(session, orientation)
                && let Ok(stored) = Layout::parse(&report.data)
            {
                keep.extend(icons::file_ids(&stored));
            }
        }
        let unused: Vec<String> = xfer::list_icons(session)
            .map_err(link)?
            .difference(&keep)
            .cloned()
            .collect();
        let mut progress = PruneProgress {
            removed: 0,
            total: unused.len() as u32,
        };
        job.progress(&progress);
        let mut removed = Vec::new();
        for id in unused {
            job.check()?;
            xfer::remove_icon(session, &id).map_err(link)?;
            removed.push(id);
            progress.removed += 1;
            job.progress(&progress);
        }
        Ok(PruneResult { removed })
    })?
}

fn feature() -> Feature {
    Feature::new("assets")
        .endpoint::<CatalogSearch>()
        .endpoint::<BuiltinList>()
        .endpoint::<Suggest>()
        .endpoint::<File>()
        .endpoint::<Create>()
        .endpoint::<Prune>()
        .job::<RenderJob>()
        .job::<PruneJob>()
}
inventory::submit! { Registration(feature) }

#[derive(Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum CatalogVariant {
    #[default]
    Base,
    Light,
    Dark,
}
#[derive(Default, Serialize, Deserialize, TS)]
pub struct CatalogQuery {
    #[serde(default)]
    pub q: String,
    pub catalog: Option<String>,
}
/// License and provenance travel with each hit for the picker and third-party notices.
#[derive(Serialize, Deserialize, TS)]
pub struct CatalogHit {
    pub source: String,
    pub name: String,
    pub id: String,
    pub aliases: Vec<String>,
    pub hex: Option<String>,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub svg: bool,
    pub light: bool,
    pub dark: bool,
    pub license: String,
    pub license_url: Option<String>,
    pub guidelines: Option<String>,
    pub attribution_url: String,
    pub attribution: String,
}
pub struct CatalogSearch;
impl Endpoint for CatalogSearch {
    const NAME: &'static str = "assets.catalog_search";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/assets/catalog/search";
    type Path = ();
    type Query = CatalogQuery;
    type Body = ();
    type Ok = Vec<CatalogHit>;
    type Error = CommonError;
    async fn handle(_: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        blocking(move || catalog::search(&req.query.q, req.query.catalog.as_deref()).into_iter().map(|e| CatalogHit {
            source: e.source.clone(), name: e.name.clone(), id: e.id.clone(), aliases: e.aliases.clone(), hex: e.hex.clone(),
            tags: e.tags.clone(), category: e.category.clone(), svg: e.svg, light: e.light, dark: e.dark,
            license: e.license.clone(), license_url: e.license_url.clone(), guidelines: e.guidelines.clone(), attribution_url: e.attribution_url.clone(),
            attribution: if e.source == "selfhst" { "Icons by selfh.st (CC BY 4.0); resized, padded or tinted. Logos belong to their owners." } else { "Icons by Simple Icons; see individual license and brand guidelines. Logos belong to their owners." }.into(),
        }).collect()).await
    }
}
