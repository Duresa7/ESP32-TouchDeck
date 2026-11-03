use super::worker::WorkerState;
use super::*;
use ::windows::{
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionMediaProperties as Properties,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Storage::Streams::{
        Buffer, DataReader, IBuffer, IRandomAccessStreamWithContentType, InputStreamOptions,
    },
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::{HSTRING, Interface, RuntimeType},
};
use std::{
    sync::{
        atomic::AtomicU32,
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
};
use windows_future::{
    AsyncOperationCompletedHandler, AsyncOperationWithProgressCompletedHandler, IAsyncInfo,
    IAsyncOperation, IAsyncOperationWithProgress,
};

const SESSION: u32 = 1;
const PROPERTIES: u32 = 2;
const PLAYBACK: u32 = 4;
const ASYNC_TIMEOUT: Duration = Duration::from_secs(2);

pub struct WindowsMedia;
struct Running {
    tx: SyncSender<Message>,
    stop: Arc<AtomicBool>,
    identity: Arc<AtomicU64>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
enum Message {
    Wake,
    Command {
        command: MediaCommand,
        session_identity: u64,
        deadline: Instant,
        reply: SyncSender<Result<bool, String>>,
    },
}
impl MediaProvider for WindowsMedia {
    fn start(&self, feed: Feed) -> Result<Arc<dyn MediaSession>, String> {
        let (tx, rx) = mpsc::sync_channel(32);
        let stop = Arc::new(AtomicBool::new(false));
        let identity = Arc::new(AtomicU64::new(1));
        let worker_identity = identity.clone();
        let worker_stop = stop.clone();
        let worker_feed = feed.clone();
        let worker_tx = tx.clone();
        let thread = thread::Builder::new()
            .name("media-mta".into())
            .spawn(move || {
                let run = || -> ::windows::core::Result<()> {
                    let _apartment = Apartment::new()?;
                    let mut request = operation(Manager::RequestAsync()?)?;
                    let manager = loop {
                        if worker_stop.load(Ordering::SeqCst) {
                            return Ok(());
                        }
                        if let Some(result) = request.poll() {
                            break result?;
                        }
                        let _ = rx.recv_timeout(Duration::from_millis(20));
                    };
                    let signals = Arc::new(Signals {
                        flags: AtomicU32::new(SESSION),
                        feed: worker_feed.clone(),
                        tx: worker_tx,
                    });
                    let mut worker =
                        Worker::new(manager, signals, worker_feed.clone(), worker_identity)?;
                    while !worker_stop.load(Ordering::SeqCst) {
                        worker.advance();
                        if let Ok(Message::Command {
                            command,
                            session_identity,
                            deadline,
                            reply,
                        }) = rx.recv_timeout(Duration::from_millis(20))
                        {
                            worker.command(command, session_identity, deadline, reply);
                        }
                    }
                    Ok(())
                };
                if run().is_err() {
                    worker_feed.error(UNAVAILABLE);
                }
            })
            .map_err(|_| UNAVAILABLE.to_owned())?;
        Ok(Arc::new(Running {
            tx,
            stop,
            identity,
            thread: Mutex::new(Some(thread)),
        }))
    }
}
impl MediaSession for Running {
    fn command(&self, command: MediaCommand, deadline: Instant) -> Receiver<Result<bool, String>> {
        let (reply, rx) = mpsc::sync_channel(1);
        let _ = self.tx.try_send(Message::Command {
            command,
            session_identity: self.identity.load(Ordering::SeqCst),
            deadline,
            reply,
        });
        rx
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.tx.try_send(Message::Wake);
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}
struct Apartment;
impl Apartment {
    fn new() -> ::windows::core::Result<Self> {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED)?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}

struct Signals {
    flags: AtomicU32,
    feed: Feed,
    tx: SyncSender<Message>,
}
impl Signals {
    fn mark(&self, flag: u32) {
        if flag != PLAYBACK {
            self.feed.invalidate();
        }
        self.flags.fetch_or(flag, Ordering::SeqCst);
        let _ = self.tx.try_send(Message::Wake);
    }
}

struct Job<T> {
    info: IAsyncInfo,
    ready: Arc<AtomicBool>,
    result: Option<Box<dyn FnOnce() -> ::windows::core::Result<T>>>,
    deadline: Instant,
}
impl<T> Job<T> {
    fn poll(&mut self) -> Option<::windows::core::Result<T>> {
        if self.ready.load(Ordering::Acquire) {
            return self.result.take().map(|result| result());
        }
        if Instant::now() >= self.deadline {
            let _ = self.info.Cancel();
            self.result = None;
            return Some(Err(::windows::core::Error::from_hresult(
                ::windows::core::HRESULT(0x800705B4u32 as i32),
            )));
        }
        None
    }
}
impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        if self.result.is_some() {
            let _ = self.info.Cancel();
        }
    }
}
fn operation<T: RuntimeType + 'static>(op: IAsyncOperation<T>) -> ::windows::core::Result<Job<T>> {
    let info = op.cast::<IAsyncInfo>()?;
    let ready = Arc::new(AtomicBool::new(false));
    let done = ready.clone();
    op.SetCompleted(&AsyncOperationCompletedHandler::new(move |_, _| {
        done.store(true, Ordering::Release);
        Ok(())
    }))?;
    Ok(Job {
        info,
        ready,
        result: Some(Box::new(move || op.GetResults())),
        deadline: Instant::now() + ASYNC_TIMEOUT,
    })
}
fn read_operation(
    op: IAsyncOperationWithProgress<IBuffer, u32>,
) -> ::windows::core::Result<Job<IBuffer>> {
    let info = op.cast::<IAsyncInfo>()?;
    let ready = Arc::new(AtomicBool::new(false));
    let done = ready.clone();
    op.SetCompleted(&AsyncOperationWithProgressCompletedHandler::new(
        move |_, _| {
            done.store(true, Ordering::Release);
            Ok(())
        },
    ))?;
    Ok(Job {
        info,
        ready,
        result: Some(Box::new(move || op.GetResults())),
        deadline: Instant::now() + ASYNC_TIMEOUT,
    })
}

struct BoundSession {
    session: Session,
    properties: Option<i64>,
    playback: Option<i64>,
}
impl BoundSession {
    fn new(session: Session, signals: &Arc<Signals>) -> ::windows::core::Result<Self> {
        let mut bound = Self {
            session,
            properties: None,
            playback: None,
        };
        let dirty = signals.clone();
        bound.properties = Some(
            bound
                .session
                .MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    dirty.mark(PROPERTIES);
                    Ok(())
                }))?,
        );
        let dirty = signals.clone();
        bound.playback = Some(bound.session.PlaybackInfoChanged(&TypedEventHandler::new(
            move |_, _| {
                dirty.mark(PLAYBACK);
                Ok(())
            },
        ))?);
        Ok(bound)
    }
}
impl Drop for BoundSession {
    fn drop(&mut self) {
        if let Some(token) = self.properties.take() {
            let _ = self.session.RemoveMediaPropertiesChanged(token);
        }
        if let Some(token) = self.playback.take() {
            let _ = self.session.RemovePlaybackInfoChanged(token);
        }
    }
}
enum Fetch {
    Properties(Job<Properties>),
    Open(Job<IRandomAccessStreamWithContentType>),
    Read {
        job: Job<IBuffer>,
        stream: IRandomAccessStreamWithContentType,
        size: u32,
    },
}
struct PendingCommand {
    job: Job<bool>,
    session_identity: u64,
    reply: SyncSender<Result<bool, String>>,
}
struct Worker {
    manager: Manager,
    manager_tokens: [Option<i64>; 2],
    signals: Arc<Signals>,
    feed: Feed,
    session: Option<BoundSession>,
    generation: u64,
    fetch: Option<Fetch>,
    commands: Vec<PendingCommand>,
    title: String,
    artist: String,
    playback: Playback,
    state: WorkerState,
}
impl Worker {
    fn new(
        manager: Manager,
        signals: Arc<Signals>,
        feed: Feed,
        identity: Arc<AtomicU64>,
    ) -> ::windows::core::Result<Self> {
        let mut worker = Self {
            manager,
            manager_tokens: [None, None],
            signals,
            generation: feed.generation(),
            state: WorkerState::new(feed.clone(), identity),
            feed,
            session: None,
            fetch: None,
            commands: Vec::new(),
            title: String::new(),
            artist: String::new(),
            playback: Playback::Idle,
        };
        let dirty = worker.signals.clone();
        worker.manager_tokens[0] = Some(worker.manager.CurrentSessionChanged(
            &TypedEventHandler::new(move |_, _| {
                dirty.mark(SESSION);
                Ok(())
            }),
        )?);
        let dirty = worker.signals.clone();
        worker.manager_tokens[1] = Some(worker.manager.SessionsChanged(
            &TypedEventHandler::new(move |_, _| {
                dirty.mark(SESSION);
                Ok(())
            }),
        )?);
        Ok(worker)
    }
    fn advance(&mut self) {
        let flags = self.signals.flags.swap(0, Ordering::SeqCst);
        if flags & SESSION != 0 {
            match self.manager.GetCurrentSession() {
                Ok(session) => {
                    if self
                        .session
                        .as_ref()
                        .is_none_or(|old| old.session != session)
                    {
                        self.state.session_changed();
                        self.fetch = None;
                        self.commands.clear();
                        self.session = None;
                        self.title.clear();
                        self.artist.clear();
                        self.playback = Playback::Idle;
                        match BoundSession::new(session, &self.signals) {
                            Ok(session) => self.session = Some(session),
                            Err(_) => {
                                self.feed.error(UNAVAILABLE);
                                return;
                            }
                        }
                        self.feed.clear_cover(self.feed.generation());
                    }
                }
                Err(error) if error.code() == ::windows::core::HRESULT(0x80004003u32 as i32) => {
                    self.fetch = None;
                    self.commands.clear();
                    self.session = None;
                    self.state.session_changed();
                    self.title.clear();
                    self.artist.clear();
                    self.playback = Playback::Idle;
                    self.feed.idle(self.feed.generation());
                }
                Err(_) => {
                    self.feed.error(UNAVAILABLE);
                    return;
                }
            }
        }
        if flags & (SESSION | PROPERTIES) != 0 {
            self.fetch = None;
            self.generation = self.feed.generation();
            self.state.properties_event(Instant::now());
            self.request_properties();
        }
        if flags & PLAYBACK != 0 {
            self.update_playback();
        }
        if self.fetch.is_none() && self.state.retry_due(Instant::now()) {
            self.request_properties();
        }
        if !self.feed.current(self.generation) {
            self.fetch = None;
        }
        self.poll_fetch();
        self.commands.retain_mut(|command| {
            if !self.state.command_current(command.session_identity) {
                let _ = command.reply.try_send(Err(NO_RESPONSE.into()));
                return false;
            }
            if let Some(result) = command.job.poll() {
                let _ = command
                    .reply
                    .try_send(result.map_err(|_| NO_RESPONSE.into()));
                return false;
            }
            true
        });
        self.feed.heartbeat(Instant::now());
    }
    fn request_properties(&mut self) {
        let Some(session) = &self.session else { return };
        match session
            .session
            .TryGetMediaPropertiesAsync()
            .and_then(operation)
        {
            Ok(job) => self.fetch = Some(Fetch::Properties(job)),
            Err(_) => self.feed.error(UNAVAILABLE),
        }
    }
    fn update_playback(&mut self) {
        let Some(session) = &self.session else { return };
        match session
            .session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
        {
            Ok(status) => {
                self.playback = match status {
                    Status::Playing => Playback::Playing,
                    Status::Paused | Status::Stopped => Playback::Paused,
                    Status::Changing => self.playback,
                    _ => Playback::Idle,
                };
                self.feed
                    .metadata(self.generation, &self.title, &self.artist, self.playback);
            }
            Err(_) => self.feed.error(UNAVAILABLE),
        }
    }
    fn poll_fetch(&mut self) {
        let Some(mut fetch) = self.fetch.take() else {
            return;
        };
        match &mut fetch {
            Fetch::Properties(job) => {
                let Some(result) = job.poll() else {
                    self.fetch = Some(fetch);
                    return;
                };
                if !self.feed.current(self.generation) {
                    return;
                }
                let Ok(properties) = result else {
                    self.feed.error(UNAVAILABLE);
                    return;
                };
                let text = || -> ::windows::core::Result<(String, String)> {
                    Ok((
                        bounded_hstring(properties.Title()?),
                        bounded_hstring(properties.Artist()?),
                    ))
                };
                let Ok((title, artist)) = text() else {
                    self.feed.error(UNAVAILABLE);
                    return;
                };
                if self.title != title || self.artist != artist {
                    self.feed.clear_cover(self.generation);
                    self.state.metadata_changed(Instant::now());
                }
                self.title = title;
                self.artist = artist;
                self.update_playback();
                match properties.Thumbnail() {
                    Ok(thumbnail) => {
                        if let Ok(job) = thumbnail.OpenReadAsync().and_then(operation) {
                            self.fetch = Some(Fetch::Open(job));
                        }
                    }
                    Err(_) => self.feed.clear_cover(self.generation),
                }
            }
            Fetch::Open(job) => {
                let Some(result) = job.poll() else {
                    self.fetch = Some(fetch);
                    return;
                };
                if !self.feed.current(self.generation) {
                    return;
                }
                let Ok(stream) = result else { return };
                let Ok(size) = stream.Size() else { return };
                if size == 0 || size > MAX_THUMBNAIL_BYTES as u64 {
                    return;
                }
                let size = size as u32;
                let read = || -> ::windows::core::Result<Job<IBuffer>> {
                    let buffer = Buffer::Create(size)?;
                    read_operation(stream.ReadAsync(&buffer, size, InputStreamOptions::None)?)
                };
                if let Ok(job) = read() {
                    self.fetch = Some(Fetch::Read { job, stream, size });
                }
            }
            Fetch::Read { job, stream, size } => {
                let Some(result) = job.poll() else {
                    self.fetch = Some(fetch);
                    return;
                };
                if !self.feed.current(self.generation) {
                    return;
                }
                let Ok(buffer) = result else { return };
                if stream.Size().ok() != Some(u64::from(*size))
                    || buffer.Length().ok() != Some(*size)
                {
                    return;
                }
                let mut bytes = vec![0; *size as usize];
                if DataReader::FromBuffer(&buffer)
                    .and_then(|reader| reader.ReadBytes(&mut bytes))
                    .is_ok()
                {
                    self.feed.cover(self.generation, &bytes);
                }
            }
        }
    }
    fn command(
        &mut self,
        command: MediaCommand,
        session_identity: u64,
        deadline: Instant,
        reply: SyncSender<Result<bool, String>>,
    ) {
        if Instant::now() >= deadline
            || !self.state.command_current(session_identity)
            || self.commands.len() >= 8
        {
            return;
        }
        let Some(session) = &self.session else { return };
        if self.manager.GetCurrentSession().ok().as_ref() != Some(&session.session) {
            return;
        }
        let result = match command {
            MediaCommand::Previous => session.session.TrySkipPreviousAsync(),
            MediaCommand::PlayPause => session.session.TryTogglePlayPauseAsync(),
            MediaCommand::Next => session.session.TrySkipNextAsync(),
        }
        .and_then(operation);
        match result {
            Ok(mut job) => {
                job.deadline = deadline;
                self.commands.push(PendingCommand {
                    job,
                    session_identity,
                    reply,
                });
            }
            Err(_) => {
                let _ = reply.try_send(Err(NO_RESPONSE.into()));
            }
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(token) = self.manager_tokens[0].take() {
            let _ = self.manager.RemoveCurrentSessionChanged(token);
        }
        if let Some(token) = self.manager_tokens[1].take() {
            let _ = self.manager.RemoveSessionsChanged(token);
        }
    }
}
fn bounded_hstring(text: HSTRING) -> String {
    let units = &*text;
    let mut result = String::new();
    for scalar in char::decode_utf16(units.iter().copied().take(MAX_TEXT_BYTES)) {
        let scalar = scalar.unwrap_or(char::REPLACEMENT_CHARACTER);
        if result.len() + scalar.len_utf8() > MAX_TEXT_BYTES {
            break;
        }
        result.push(scalar);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a real Windows media session"]
    fn media_os_smoke() {
        let media = Media::system();
        thread::sleep(Duration::from_secs(3));
        assert_eq!(media.summary().status.state, "ok");
        let samples = media.live();
        let state = samples.iter().find(|s| s.stream == "state").unwrap();
        let has_cover = matches!(
            samples
                .iter()
                .find(|s| s.stream == "cover")
                .map(|s| &s.value),
            Some(LiveValue::Image(Some(_)))
        );
        println!(
            "GSMTC initialized; playback={:?}; thumbnail={has_cover}",
            state.value
        );
        if state.value != LiveValue::Number(0.0) {
            let control = media.control().unwrap();
            let first = control.act("", "play_pause", ASYNC_TIMEOUT);
            thread::sleep(Duration::from_millis(500));
            let restore = control.act("", "play_pause", ASYNC_TIMEOUT);
            assert!(first.is_ok() && restore.is_ok());
            println!("Play/pause succeeded twice; original playback restored.");
        } else {
            println!("No current player; command and cover smoke remain for the combined pass.");
        }
    }
}
