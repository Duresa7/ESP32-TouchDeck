//! One Device's picture stream: the next picture from the folder, sent at the Device's pace.
//!
//! The Device decides when a picture may come. While it is awake every put answers `idle`, so
//! the stream stops sending and asks again once a second. Inside the interval a put answers
//! `rate` with the wait. `busy` means another transfer holds the slot, and the stream backs
//! off. A picture the Device can't take, or one that can't be read here, is skipped.
use super::{encode, folder};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};
use td_link::{
    AssetReason, LinkError,
    photo::{PhotoError, PhotoShown},
};

/// The contract's slowest-allowed retry while the Device is awake.
pub(crate) const IDLE_RETRY: Duration = Duration::from_secs(1);
const BUSY_FIRST: Duration = Duration::from_millis(500);
const BUSY_MAX: Duration = Duration::from_secs(8);
const FAIL_FIRST: Duration = Duration::from_secs(1);
const FAIL_MAX: Duration = Duration::from_secs(30);
const RATE_MIN: Duration = Duration::from_millis(100);
const RATE_MAX: Duration = Duration::from_secs(3600);
/// How often an empty folder is looked at again.
pub(crate) const EMPTY_RETRY: Duration = Duration::from_secs(10);
/// Failed sends of one picture before it is skipped.
const TRIES: u32 = 3;

/// What the Device takes: its screen, its `photo_bytes` and its `interval_s`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Target {
    pub screen: [u32; 2],
    pub limit: usize,
    pub interval: Duration,
}

pub(crate) struct Streamer {
    folder: PathBuf,
    queue: Vec<PathBuf>,
    ready: Option<(PathBuf, [u32; 2], Vec<u8>)>,
    skipped: HashSet<PathBuf>,
    busy: Duration,
    failures: u32,
    fail_wait: Duration,
}

impl Streamer {
    pub(crate) fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            queue: Vec::new(),
            ready: None,
            skipped: HashSet::new(),
            busy: BUSY_FIRST,
            failures: 0,
            fail_wait: FAIL_FIRST,
        }
    }

    pub(crate) fn folder(&self) -> &Path {
        &self.folder
    }

    /// The next picture, encoded for the target. `None` when the folder has none to show.
    fn next(&mut self, target: &Target) -> Option<&[u8]> {
        if self
            .ready
            .as_ref()
            .is_some_and(|(_, screen, jpeg)| *screen != target.screen || jpeg.len() > target.limit)
        {
            self.ready = None;
        }
        let mut refilled = false;
        while self.ready.is_none() {
            if self.queue.is_empty() {
                if refilled {
                    return None;
                }
                refilled = true;
                self.queue = folder::pictures(&self.folder)
                    .into_iter()
                    .filter(|p| !self.skipped.contains(p))
                    .rev()
                    .collect();
                continue;
            }
            let path = self.queue.pop().expect("the queue is not empty");
            match folder::read(&self.folder, &path)
                .map_err(|e| e.to_string())
                .and_then(|file| encode::prepare(&file, target.screen, target.limit))
            {
                Ok(jpeg) => self.ready = Some((path, target.screen, jpeg)),
                Err(_) => {
                    self.skipped.insert(path);
                }
            }
        }
        self.ready.as_ref().map(|(_, _, jpeg)| jpeg.as_slice())
    }

    fn skip(&mut self) {
        if let Some((path, _, _)) = self.ready.take() {
            self.skipped.insert(path);
        }
        self.failures = 0;
        self.fail_wait = FAIL_FIRST;
    }

    /// Sends the next picture through `send` and says how long to wait before the next step.
    pub(crate) fn step(
        &mut self,
        target: &Target,
        send: impl FnOnce(&[u8]) -> Result<PhotoShown, PhotoError>,
    ) -> Duration {
        let Some(jpeg) = self.next(target) else {
            return EMPTY_RETRY;
        };
        let outcome = send(jpeg);
        if !matches!(
            outcome,
            Err(PhotoError::Refused {
                reason: AssetReason::Busy,
                ..
            } | PhotoError::Link(LinkError::TransferBusy))
        ) {
            self.busy = BUSY_FIRST;
        }
        match outcome {
            Ok(_) => {
                self.ready = None;
                self.failures = 0;
                self.fail_wait = FAIL_FIRST;
                target.interval
            }
            Err(PhotoError::Refused {
                reason: AssetReason::Idle,
                ..
            }) => IDLE_RETRY,
            Err(PhotoError::Refused {
                reason: AssetReason::Rate,
                retry_ms,
                ..
            }) => retry_ms
                .map_or(target.interval, Duration::from_millis)
                .clamp(RATE_MIN, RATE_MAX),
            Err(
                PhotoError::Refused {
                    reason: AssetReason::Busy,
                    ..
                }
                | PhotoError::Link(LinkError::TransferBusy),
            ) => {
                let wait = self.busy;
                self.busy = (self.busy * 2).min(BUSY_MAX);
                wait
            }
            Err(
                PhotoError::Refused {
                    reason: AssetReason::Format | AssetReason::Size,
                    ..
                }
                | PhotoError::Size { .. },
            ) => {
                self.skip();
                Duration::ZERO
            }
            Err(_) => {
                self.failures += 1;
                let wait = self.fail_wait;
                self.fail_wait = (self.fail_wait * 2).min(FAIL_MAX);
                if self.failures >= TRIES {
                    self.skip();
                }
                wait
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photos::encode::tests::noise_png;
    use td_link::photo::{IdleSettings, PHOTO_BYTES_MAX, idle_set, put_photo};
    use td_link::{MockDeck, Session, SessionConfig};

    const TARGET: Target = Target {
        screen: [160, 100],
        limit: PHOTO_BYTES_MAX,
        interval: Duration::from_secs(15),
    };

    fn session(deck: &MockDeck) -> Session {
        let session = Session::open(
            Box::new(deck.connect()),
            SessionConfig {
                ping_every: None,
                ..SessionConfig::default()
            },
        )
        .unwrap()
        .0;
        session.identity(Duration::from_secs(1)).unwrap();
        idle_set(
            &session,
            IdleSettings {
                enabled: true,
                ..IdleSettings::default()
            },
        )
        .unwrap();
        session
    }

    fn folder(pictures: u32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..pictures {
            std::fs::write(
                dir.path().join(format!("{n}.png")),
                noise_png(200, 120, n + 1),
            )
            .unwrap();
        }
        dir
    }

    fn send(session: &Session) -> impl FnOnce(&[u8]) -> Result<PhotoShown, PhotoError> + '_ {
        move |jpeg| put_photo(session, jpeg, TARGET.limit)
    }

    #[test]
    fn the_stream_backs_off_on_busy_and_waits_out_rate_before_the_picture_goes() {
        let deck = MockDeck::v2();
        let session = session(&deck);
        deck.set_idle(true);
        deck.refuse_photos([
            ("busy", None),
            ("busy", None),
            ("busy", None),
            ("rate", Some(300)),
        ]);
        let dir = folder(2);
        let mut stream = Streamer::new(dir.path().to_owned());
        let waits: Vec<Duration> = (0..5)
            .map(|_| stream.step(&TARGET, send(&session)))
            .collect();
        assert_eq!(
            waits,
            [
                Duration::from_millis(500),
                Duration::from_millis(1000),
                Duration::from_millis(2000),
                Duration::from_millis(300),
                TARGET.interval,
            ]
        );
        assert_eq!(deck.photos().len(), 1);
        assert_eq!(
            stream.step(&TARGET, |_| Err(PhotoError::Link(LinkError::TransferBusy))),
            Duration::from_millis(500),
            "a success starts the busy backoff over"
        );
    }

    #[test]
    fn an_awake_device_stops_the_stream_until_it_is_idle_again() {
        let deck = MockDeck::v2();
        let session = session(&deck);
        deck.set_photo_interval(Duration::ZERO);
        let dir = folder(3);
        let mut stream = Streamer::new(dir.path().to_owned());
        for _ in 0..3 {
            assert_eq!(stream.step(&TARGET, send(&session)), IDLE_RETRY);
        }
        assert_eq!(
            deck.chunk_bytes(),
            0,
            "an awake Device gets no picture bytes"
        );
        deck.set_idle(true);
        for _ in 0..4 {
            assert_eq!(stream.step(&TARGET, send(&session)), TARGET.interval);
        }
        let shown = deck.photos();
        assert_eq!(shown.len(), 4);
        assert_eq!(
            shown[0], shown[3],
            "the fourth picture starts the folder over"
        );
        assert_ne!(shown[0], shown[1]);
        deck.set_idle(false);
        assert_eq!(stream.step(&TARGET, send(&session)), IDLE_RETRY);
        assert_eq!(deck.photos().len(), 4);
    }

    #[test]
    fn unreadable_and_refused_pictures_are_skipped() {
        let deck = MockDeck::v2();
        let session = session(&deck);
        deck.set_idle(true);
        deck.set_photo_interval(Duration::ZERO);
        let dir = folder(1);
        std::fs::write(dir.path().join("broken.jpg"), b"not a jpeg").unwrap();
        std::fs::write(dir.path().join("2.png"), noise_png(90, 60, 9)).unwrap();
        let mut stream = Streamer::new(dir.path().to_owned());
        assert_eq!(stream.step(&TARGET, send(&session)), TARGET.interval);
        deck.refuse_photos([("format", None)]);
        assert_eq!(stream.step(&TARGET, send(&session)), Duration::ZERO);
        assert_eq!(stream.step(&TARGET, send(&session)), TARGET.interval);
        assert_eq!(stream.step(&TARGET, send(&session)), TARGET.interval);
        assert_eq!(deck.photos().len(), 3);
        assert!(deck.photos().windows(2).all(|w| w[0] == w[1]));
        assert_eq!(stream.skipped.len(), 2, "broken.jpg and the refused 2.png");
    }

    #[test]
    fn every_picture_sent_fits_the_device_limit_and_screen() {
        let deck = MockDeck::v2();
        let session = session(&deck);
        deck.set_idle(true);
        deck.set_photo_interval(Duration::ZERO);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("big.png"), noise_png(640, 400, 4)).unwrap();
        let target = Target {
            limit: 6000,
            ..TARGET
        };
        let mut stream = Streamer::new(dir.path().to_owned());
        assert_eq!(
            stream.step(&target, |jpeg| put_photo(&session, jpeg, target.limit)),
            target.interval
        );
        let sent = &deck.photos()[0];
        assert!(sent.len() <= target.limit, "{}", sent.len());
        let picture = image::load_from_memory(sent).unwrap();
        assert!(picture.width() <= 160 && picture.height() <= 100);
    }

    #[test]
    fn an_empty_folder_waits_without_sending() {
        let deck = MockDeck::v2();
        let session = session(&deck);
        deck.set_idle(true);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"no pictures").unwrap();
        let mut stream = Streamer::new(dir.path().to_owned());
        assert_eq!(stream.step(&TARGET, send(&session)), EMPTY_RETRY);
        assert!(deck.received().iter().all(|m| m["m"] != "v2.photo.put"));
    }
}
