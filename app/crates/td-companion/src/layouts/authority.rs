//! Device Layout authority: controls resolve only from the Pushed copy on disk.
use crate::{
    controls::Settable,
    devices::{ControlLayout, ControlLayoutUnavailable, DeviceManager, LinkState, ShownLayout},
    live::{LiveCaps, stream_source},
    store::LayoutStore,
};
use parking_lot::Mutex;
use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
use td_core::{Layout, layout::Tile};

#[derive(Clone)]
pub struct LayoutAuthority(Arc<State>);
struct State {
    generation: AtomicU64,
    publication: Mutex<()>,
    devices: DeviceManager,
    pushed: LayoutStore,
    settable: Settable,
    changed: Mutex<BTreeSet<String>>,
    shown: Mutex<HashMap<String, (u64, Arc<Layout>)>>,
    waker: OnceLock<Arc<dyn Fn() + Send + Sync>>,
}
#[derive(Debug)]
pub enum ReplaceError<E> {
    Write(E),
    Save(std::io::Error),
}
impl State {
    fn assess(&self, device: &str, session: u64) -> (ControlLayout, Option<Layout>) {
        let changed = self.changed.lock();
        if changed.contains(device) {
            return (ControlLayout::default(), None);
        }
        let layout = match self.shown.lock().get(device) {
            Some((read_in, layout)) if *read_in == session => layout.clone(),
            _ => return (ControlLayout::default(), None),
        };
        let orientation = layout.orientation;
        let has_controls = layout.pages.iter().any(|page| {
            let td_core::layout::Page::Grid { tiles, .. } = page else {
                return false;
            };
            tiles.iter().any(|tile| {
                ["previous", "play_pause", "next", "mute"]
                    .iter()
                    .any(|part| td_host::actions::Plan::part(tile, part).is_some())
                    || tile.streams.iter().flatten().any(|(stream, _)| {
                        crate::controls::takes_input(tile, stream)
                            && stream_source(tile, stream)
                                .is_some_and(|source| (self.settable)(&source))
                    })
            })
        });
        let Some(trusted) = self.pushed.get(device, layout.orientation) else {
            return (
                ControlLayout::NoPushedCopy {
                    orientation,
                    has_controls,
                },
                None,
            );
        };
        if trusted.to_canonical_json() != layout.to_canonical_json() {
            return (
                ControlLayout::Differs {
                    orientation,
                    has_controls,
                },
                None,
            );
        }
        (
            ControlLayout::Matches {
                orientation,
                has_controls,
            },
            Some(trusted),
        )
    }
}
impl LayoutAuthority {
    pub(crate) fn new(pushed: LayoutStore, settable: Settable, devices: DeviceManager) -> Self {
        Self(Arc::new(State {
            pushed,
            settable,
            devices,
            generation: AtomicU64::new(0),
            publication: Mutex::new(()),
            changed: Mutex::default(),
            shown: Mutex::default(),
            waker: OnceLock::new(),
        }))
    }
    /// Connects replacement notifications to the existing Live round wakeup.
    pub(crate) fn on_change(&self, wake: Arc<dyn Fn() + Send + Sync>) {
        let _ = self.0.waker.set(wake);
    }
    /// Invalidates before writing, without holding authority locks during the write.
    /// A push (`Some`) keeps the original single invalidation: invalidating again can
    /// erase a failed read and silently retry it. An orientation write (`None`) also
    /// invalidates afterwards, including when the caller captured a failed request.
    /// Only a successful save changes the Pushed copy; assessment always reads disk.
    pub fn replace<E>(
        &self,
        device: &str,
        write: impl FnOnce() -> Result<Option<Layout>, E>,
    ) -> Result<(), ReplaceError<E>> {
        self.invalidate(device);
        match write().map_err(ReplaceError::Write)? {
            Some(layout) => self
                .0
                .pushed
                .put(device, &layout)
                .map_err(ReplaceError::Save),
            None => {
                self.invalidate(device);
                Ok(())
            }
        }
    }
    fn invalidate(&self, device: &str) {
        let _publication = self.0.publication.lock();
        let mut changed = self.0.changed.lock();
        changed.insert(device.to_owned());
        self.0
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.0.shown.lock().remove(device);
        drop(changed);
        {
            let devices = &self.0.devices;
            let session = devices
                .summaries()
                .into_iter()
                .find(|d| d.id.as_str() == device)
                .and_then(|d| match d.link {
                    LinkState::Connected { since_ms, .. } => Some(since_ms),
                    _ => None,
                });
            devices.control_layout(device, session, ControlLayout::default());
        }
        if let Some(wake) = self.0.waker.get() {
            wake();
        }
    }

    /// Global for compatibility. Truly per-Device generations are a later internal change.
    pub fn generation(&self, _device: &str) -> u64 {
        self.0.generation.load(Ordering::SeqCst)
    }
    /// Returns a Tile from the Companion's Pushed copy only when the active Layout matches in this session.
    /// Returns None during replacement or before a matching Layout has been read. Never waits for a link.
    pub fn tile(&self, device: &str, session: u64, page: &str, tile: &str) -> Option<Tile> {
        self.binding(device, session, page, tile).ok().flatten()
    }

    pub(crate) fn binding(
        &self,
        device: &str,
        session: u64,
        page: &str,
        tile: &str,
    ) -> Result<Option<Tile>, ()> {
        let (_, trusted) = self.0.assess(device, session);
        let trusted = trusted.ok_or(())?;
        Ok(td_host::actions::find_tile(&trusted, page, tile).cloned())
    }

    /// Starts the single Live round and hands off Devices whose Layouts need re-reading.
    pub(crate) fn begin_round(&self) -> (u64, BTreeSet<String>) {
        let generation = self.generation("");
        let mut changed = self.0.changed.lock();
        let mut shown = self.0.shown.lock();
        for device in changed.iter() {
            shown.remove(device);
        }
        (generation, std::mem::take(&mut *changed))
    }
    /// Accepts router reads without doing link I/O. Keep shown-copy filtering separate
    /// from generation-guarded Device summaries, as in the original Live publication.
    pub(crate) fn publish(
        &self,
        generation: u64,
        shown: HashMap<String, (u64, Arc<Layout>)>,
        layouts: Vec<(String, u64, ShownLayout)>,
        failed: BTreeSet<(String, u64)>,
    ) {
        publish_shown(&self.0, shown);
        publish_layouts(&self.0, &self.0.devices, layouts, generation);
        publish_status(&self.0, &self.0.devices, generation, &failed);
    }
}
fn publish_status(
    shared: &State,
    devices: &DeviceManager,
    generation: u64,
    failed: &BTreeSet<(String, u64)>,
) {
    let _publication = shared.publication.lock();
    if shared.generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
        return;
    }
    for device in devices.summaries() {
        let (session, status) = match device.link {
            LinkState::Connected {
                since_ms,
                protocol: 2,
                ..
            } if device
                .capabilities
                .as_ref()
                .and_then(LiveCaps::of)
                .is_some() =>
            {
                let mut status = shared.assess(&device.id, since_ms).0;
                if matches!(status, ControlLayout::Unavailable { .. })
                    && failed.contains(&(device.id.to_string(), since_ms))
                {
                    status = ControlLayout::Unavailable {
                        reason: ControlLayoutUnavailable::ReadFailed,
                    };
                }
                (Some(since_ms), status)
            }
            LinkState::Connected { since_ms, .. } => (
                Some(since_ms),
                ControlLayout::Unavailable {
                    reason: ControlLayoutUnavailable::Unsupported,
                },
            ),
            _ => (
                None,
                ControlLayout::Unavailable {
                    reason: ControlLayoutUnavailable::Offline,
                },
            ),
        };
        devices.control_layout(&device.id, session, status);
    }
}

fn publish_layouts(
    shared: &State,
    devices: &DeviceManager,
    layouts: Vec<(String, u64, ShownLayout)>,
    generation: u64,
) {
    let _publication = shared.publication.lock();
    if shared.generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
        return;
    }
    let changed = shared.changed.lock().clone();
    for (device, session, shown) in layouts {
        if !changed.contains(&device) {
            devices.layout_shown(&device, session, shown);
        }
    }
}

fn publish_shown(shared: &State, mut shown: HashMap<String, (u64, Arc<Layout>)>) {
    let changed = shared.changed.lock();
    shown.retain(|device, _| !changed.contains(device));
    *shared.shown.lock() = shown;
}

#[cfg(test)]
mod existing_tests {
    use super::*;
    use std::{
        thread,
        time::{Duration, Instant},
    };
    fn test_authority(settable: Settable) -> LayoutAuthority {
        LayoutAuthority::new(
            LayoutStore::new(tempfile::tempdir().unwrap().path().to_owned()),
            settable,
            DeviceManager::new(crate::api::Events::default()),
        )
    }
    #[test]
    fn a_tile_comes_from_the_shown_layout_of_the_same_session_and_never_from_a_replaced_one() {
        let devices = DeviceManager::new(crate::api::Events::default());

        let layout =
            Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
        let data = tempfile::tempdir().unwrap();
        let store = crate::store::LayoutStore::new(data.path().to_owned());
        store.put("d", &layout).unwrap();
        let live = LayoutAuthority::new(store, Arc::new(|_| false), devices.clone());
        live.0
            .shown
            .lock()
            .insert("d".into(), (7, Arc::new(layout)));
        let tile = live.tile("d", 7, "media", "volume").unwrap();
        assert!(tile.input);
        assert_eq!(live.tile("d", 7, "media", "nope"), None);
        assert_eq!(live.tile("d", 7, "other", "volume"), None);
        assert_eq!(live.tile("d", 8, "media", "volume"), None);
        assert_eq!(live.tile("e", 7, "media", "volume"), None);

        let router = Mutex::new(crate::live::Router::default());
        let busy = router.lock();
        live.invalidate("d");
        assert_eq!(live.tile("d", 7, "media", "volume"), None);
        drop(busy);
    }

    #[test]
    fn a_round_cannot_restore_a_layout_invalidated_during_delivery() {
        let devices = DeviceManager::new(crate::api::Events::default());

        let layout =
            Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
        let data = tempfile::tempdir().unwrap();
        let store = crate::store::LayoutStore::new(data.path().to_owned());
        store.put("d", &layout).unwrap();
        let live = LayoutAuthority::new(store, Arc::new(|_| false), devices.clone());
        let old_round = HashMap::from([("d".into(), (7, Arc::new(layout)))]);
        live.invalidate("d");
        publish_shown(&live.0, old_round);
        live.0.changed.lock().clear();
        assert!(live.tile("d", 7, "media", "volume").is_none());
    }

    #[test]
    fn control_layout_assessment_tracks_authority_and_orientation() {
        use crate::devices::ControlLayout;
        let devices = DeviceManager::new(crate::api::Events::default());
        for orientation in [
            td_core::Orientation::Landscape,
            td_core::Orientation::Portrait,
        ] {
            let mut layout =
                Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
            layout.orientation = orientation;
            let data = tempfile::tempdir().unwrap();
            let store = crate::store::LayoutStore::new(data.path().to_owned());
            let live = LayoutAuthority::new(store.clone(), Arc::new(|_| false), devices.clone());
            live.0
                .shown
                .lock()
                .insert("d".into(), (7, Arc::new(layout.clone())));
            assert!(matches!(
                live.0.assess("d", 7).0,
                ControlLayout::NoPushedCopy {
                    has_controls: true,
                    ..
                }
            ));
            assert!(live.tile("d", 7, "media", "volume").is_none());
            store.put("d", &layout).unwrap();
            assert!(matches!(
                live.0.assess("d", 7).0,
                ControlLayout::Matches { .. }
            ));
            let path = data
                .path()
                .join("d")
                .join(format!("{}.json", crate::store::name(orientation)));
            std::fs::write(&path, serde_json::to_vec_pretty(&layout).unwrap()).unwrap();
            assert!(matches!(
                live.0.assess("d", 7).0,
                ControlLayout::Matches { .. }
            ));
            layout.name = "Changed".into();
            live.0
                .shown
                .lock()
                .insert("d".into(), (7, Arc::new(layout)));
            assert!(matches!(
                live.0.assess("d", 7).0,
                ControlLayout::Differs { .. }
            ));
            std::fs::write(&path, b"broken").unwrap();
            assert!(matches!(
                live.0.assess("d", 7).0,
                ControlLayout::NoPushedCopy { .. }
            ));
            std::fs::remove_file(path).unwrap();
            assert!(matches!(
                live.0.assess("d", 7).0,
                ControlLayout::NoPushedCopy { .. }
            ));
            assert!(matches!(
                live.0.assess("d", 8).0,
                ControlLayout::Unavailable { .. }
            ));
            live.invalidate("d");
            assert!(matches!(
                live.0.assess("d", 7).0,
                ControlLayout::Unavailable { .. }
            ));
        }
    }

    #[test]
    fn control_layout_without_controls_including_builtin() {
        let live = test_authority(Arc::new(|_| false));

        let builtin = Layout::parse(include_bytes!(
            "../../../../../schema/examples/builtin.json"
        ))
        .unwrap();
        live.0
            .shown
            .lock()
            .insert("d".into(), (1, Arc::new(builtin)));
        assert!(matches!(
            live.0.assess("d", 1).0,
            ControlLayout::NoPushedCopy {
                has_controls: false,
                ..
            }
        ));
    }

    #[test]
    fn control_layout_publication_rejects_a_round_before_invalidation() {
        use crate::devices::{BoardKind, Config, DeviceId};
        let devices = DeviceManager::new(crate::api::Events::default());
        let config = Config::mock(None);
        config
            .mock
            .as_ref()
            .unwrap()
            .0
            .plug("mock0001", BoardKind::Device);
        devices.start(config);
        let id = DeviceId::parse("mock0001").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let session = loop {
            if let Some(d) = devices.get(&id)
                && let LinkState::Connected { since_ms, .. } = d.link
            {
                break since_ms;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        };

        let data = tempfile::tempdir().unwrap();
        let store = crate::store::LayoutStore::new(data.path().to_owned());
        let layout =
            Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
        store.put(&id, &layout).unwrap();
        let live = LayoutAuthority::new(store, Arc::new(|_| false), devices.clone());
        live.0
            .shown
            .lock()
            .insert(id.to_string(), (session, Arc::new(layout.clone())));
        let old = live.generation("d");
        publish_status(&live.0, &devices, old, &BTreeSet::new());
        assert!(matches!(
            devices.get(&id).unwrap().control_layout,
            ControlLayout::Matches { .. }
        ));
        let rev = devices.get(&id).unwrap().rev;
        publish_status(&live.0, &devices, old, &BTreeSet::new());
        assert_eq!(devices.get(&id).unwrap().rev, rev);
        live.invalidate(&id);
        live.0.changed.lock().clear();
        live.0
            .shown
            .lock()
            .insert(id.to_string(), (session, Arc::new(layout)));
        publish_status(&live.0, &devices, old, &BTreeSet::new());
        assert_eq!(
            devices.get(&id).unwrap().control_layout,
            ControlLayout::default()
        );
        devices.disconnect(&id);
        devices.control_layout(
            &id,
            Some(session),
            ControlLayout::Matches {
                orientation: td_core::Orientation::Landscape,
                has_controls: true,
            },
        );
        assert_eq!(
            devices.get(&id).unwrap().control_layout,
            ControlLayout::default()
        );
        devices.stop();
    }

    #[test]
    fn control_layout_bar_uses_the_touch_settable_predicate() {
        let live = test_authority(Arc::new(|key| {
            key.integration == "volume" && key.stream == "level"
        }));
        let mut layout =
            Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
        let td_core::layout::Page::Grid { tiles, .. } = &mut layout.pages[0] else {
            panic!("grid")
        };
        tiles.retain(|tile| tile.id == "volume");
        tiles[0].streams.as_mut().unwrap().remove("muted");
        for (input, expected) in [(false, false), (true, true)] {
            let td_core::layout::Page::Grid { tiles, .. } = &mut layout.pages[0] else {
                panic!("grid")
            };
            tiles[0].input = input;
            live.0
                .shown
                .lock()
                .insert("d".into(), (1, Arc::new(layout.clone())));
            assert_eq!(
                live.0.assess("d", 1).0,
                ControlLayout::NoPushedCopy {
                    orientation: layout.orientation,
                    has_controls: expected,
                }
            );
        }
        let readonly = test_authority(Arc::new(|_| false));
        readonly
            .0
            .shown
            .lock()
            .insert("d".into(), (1, Arc::new(layout.clone())));
        assert_eq!(
            readonly.0.assess("d", 1).0,
            ControlLayout::NoPushedCopy {
                orientation: layout.orientation,
                has_controls: false,
            }
        );
    }
}

#[cfg(test)]
mod interface_tests {
    use super::*;
    use crate::devices::{BoardKind, Config, DeviceId};
    use std::time::{Duration, Instant};
    const DEVICE: &str = "mock0001";
    struct Fixture {
        authority: LayoutAuthority,
        devices: DeviceManager,
        data: tempfile::TempDir,
        session: u64,
        layout: Layout,
    }
    impl Fixture {
        fn new() -> Self {
            let data = tempfile::tempdir().unwrap();
            let devices = DeviceManager::new(crate::api::Events::default());
            let config = Config::mock(None);
            config
                .mock
                .as_ref()
                .unwrap()
                .0
                .plug(DEVICE, BoardKind::Device);
            devices.start(config);
            let deadline = Instant::now() + Duration::from_secs(5);
            let session = loop {
                if let Some(summary) = devices.get(&DeviceId::parse(DEVICE).unwrap())
                    && let LinkState::Connected { since_ms, .. } = summary.link
                {
                    break since_ms;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            };
            let authority = LayoutAuthority::new(
                LayoutStore::new(data.path().to_owned()),
                Arc::new(|_| true),
                devices.clone(),
            );
            let layout =
                Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
            Self {
                authority,
                devices,
                data,
                session,
                layout,
            }
        }
        fn publish(&self, generation: u64, layout: &Layout) {
            self.authority.publish(
                generation,
                HashMap::from([(DEVICE.into(), (self.session, Arc::new(layout.clone())))]),
                vec![],
                BTreeSet::new(),
            );
        }
        fn read(&self, layout: &Layout) {
            let (generation, _) = self.authority.begin_round();
            self.publish(generation, layout);
        }
        fn status(&self) -> ControlLayout {
            self.devices
                .get(&DeviceId::parse(DEVICE).unwrap())
                .unwrap()
                .control_layout
        }
        fn tile(&self) -> Option<Tile> {
            self.authority.tile(DEVICE, self.session, "media", "volume")
        }
        fn save(&self) {
            self.authority
                .replace(DEVICE, || Ok::<_, ()>(Some(self.layout.clone())))
                .unwrap();
        }
        fn block_save(&self) {
            // write_atomic must create this temporary file beside the existing Pushed copy.
            // A directory here fails the write while leaving the old copy readable.
            std::fs::create_dir_all(
                self.data
                    .path()
                    .join(DEVICE)
                    .join(format!(".landscape.json.{}.tmp", std::process::id())),
            )
            .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.devices.stop();
        }
    }

    #[test]
    fn overlapping_read_and_orientation_replace_require_a_post_replace_read() {
        let f = Fixture::new();
        f.save();
        f.read(&f.layout);
        let expected = td_host::actions::find_tile(&f.layout, "media", "volume").cloned();
        assert_eq!(f.tile(), expected);
        let (old, _) = f.authority.begin_round();
        let before = f.authority.generation(DEVICE);
        f.authority
            .replace(DEVICE, || {
                assert_eq!(f.status(), ControlLayout::default());
                assert!(f.tile().is_none());
                assert_eq!(f.authority.generation(DEVICE), before + 1);
                f.publish(old, &f.layout);
                assert!(f.tile().is_none());
                Ok::<_, ()>(None)
            })
            .unwrap();
        assert_eq!(f.authority.generation(DEVICE), before + 2);
        f.publish(old, &f.layout);
        assert_eq!(f.status(), ControlLayout::default());
        assert!(f.tile().is_none());
        f.read(&f.layout);
        assert_eq!(f.tile(), expected);
        assert!(
            f.authority
                .tile(DEVICE, f.session + 1, "media", "volume")
                .is_none()
        );
    }

    #[test]
    fn failed_save_compares_with_the_copy_still_on_disk() {
        for identical in [false, true] {
            let f = Fixture::new();
            f.save();
            f.block_save();
            let mut sent = f.layout.clone();
            if !identical {
                sent.name = "Replacement".into();
            }
            assert!(matches!(
                f.authority
                    .replace(DEVICE, || Ok::<_, ()>(Some(sent.clone()))),
                Err(ReplaceError::Save(_))
            ));
            f.read(&sent);
            if identical {
                assert!(matches!(f.status(), ControlLayout::Matches { .. }));
                assert!(f.tile().is_some());
            } else {
                assert!(matches!(f.status(), ControlLayout::Differs { .. }));
                assert!(f.tile().is_none());
            }
        }
        let f = Fixture::new();
        std::fs::write(f.data.path().join(DEVICE), b"blocked").unwrap();
        assert!(matches!(
            f.authority
                .replace(DEVICE, || Ok::<_, ()>(Some(f.layout.clone()))),
            Err(ReplaceError::Save(_))
        ));
        f.read(&f.layout);
        assert!(matches!(f.status(), ControlLayout::NoPushedCopy { .. }));
        assert!(f.tile().is_none());
    }

    #[test]
    fn stale_status_publication_is_dropped_even_after_changed_is_drained() {
        let f = Fixture::new();
        f.save();
        f.read(&f.layout);
        let (old, _) = f.authority.begin_round();
        f.authority.replace(DEVICE, || Ok::<_, ()>(None)).unwrap();
        let (current, _) = f.authority.begin_round();
        f.publish(old, &f.layout);
        assert_eq!(f.status(), ControlLayout::default());
        f.publish(current, &f.layout);
        assert!(matches!(f.status(), ControlLayout::Matches { .. }));
    }
}
