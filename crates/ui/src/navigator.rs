//! Archive-backed URL resolution for Scaleform movie dependencies.
//!
//! Scaleform menus commonly use `ImportAssets` with a URL relative to the
//! menu SWF (for example, Fallout 4's `interface\hudmenu.swf` imports
//! `fonts_en.swf`). Ruffle delegates those loads to `NavigatorBackend`.
//! This module maps the virtual file URL back to a Gamebryo archive path and
//! keeps Ruffle's local future executor available to the owning player.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use byroredux_bsa::{Ba2Archive, BsaArchive};
use ruffle_core::backend::navigator::{
    ErrorResponse, NavigationMethod, NavigatorBackend, NullExecutor, NullSpawner, OwnedFuture,
    Request, SuccessResponse,
};
use ruffle_core::indexmap::IndexMap;
use ruffle_core::loader::Error;
use ruffle_core::socket::{ConnectionState, SocketAction, SocketHandle};
use ruffle_core::swf::Encoding;
use swf::{Tag, TagCode};
use url::{ParseError, Url};

/// Supplies files addressed by normalized Gamebryo archive paths.
///
/// Paths use backslashes and are relative to the game data root, such as
/// `interface\fonts_en.swf`. Returning `Ok(None)` means the source does not
/// contain the requested resource.
///
/// `Send + Sync` (#2734): the provider is held behind an `Arc` so the archive
/// extract can move off the main loop to a worker. `BsaArchive`/`Ba2Archive`
/// already satisfy this — each serialises its `File` cursor behind a `Mutex`
/// so `extract` can take `&self` (#360) — so the bound costs nothing today
/// and is what makes sharing the streaming worker's provider possible rather
/// than a fresh archive handle per menu.
pub trait ScaleformResourceProvider: Send + Sync {
    fn load(&self, path: &str) -> io::Result<Option<Vec<u8>>>;
}

impl ScaleformResourceProvider for Ba2Archive {
    fn load(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        if self.contains(path) {
            self.extract(path).map(Some)
        } else {
            Ok(None)
        }
    }
}

impl ScaleformResourceProvider for BsaArchive {
    fn load(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        if self.contains(path) {
            self.extract(path).map(Some)
        } else {
            Ok(None)
        }
    }
}

/// One successful dependency load performed for a Scaleform movie.
///
/// One raw event per fetch — the navigator layer does not deduplicate;
/// [`crate::SwfPlayer::resource_loads`] is the deduplicated, capped,
/// hit-counted view (#2967).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaleformResourceLoad {
    pub request_url: String,
    pub archive_path: String,
    pub byte_len: usize,
    /// Ruffle currently starts `ImportAssets` children at preload frame zero.
    /// This is true when a leading frame boundary was synthesized to keep
    /// AVM2 bytecode and symbol tags associated with their intended frame.
    pub import_preload_rewritten: bool,
    /// Number of times this `archive_path` has been fetched, as counted by
    /// [`crate::SwfPlayer::resource_loads`]'s dedup (#2967). Always `1` on a
    /// raw event straight from the navigator — a repeat fetch is a distinct
    /// event here, and only becomes a bumped counter once merged.
    pub hit_count: u64,
}

#[derive(Default)]
struct NavigatorState {
    loads: Vec<ScaleformResourceLoad>,
    errors: Vec<String>,
    import_asset_paths: HashSet<String>,
}

pub(crate) struct ScaleformNavigatorRuntime {
    executor: NullExecutor,
    state: Rc<RefCell<NavigatorState>>,
}

impl ScaleformNavigatorRuntime {
    pub(crate) fn create(
        movie_path: &str,
        movie_data: &[u8],
        provider: Arc<dyn ScaleformResourceProvider>,
    ) -> Result<(ScaleformNavigator, Self, String), String> {
        let movie_url = archive_movie_url(movie_path)?;
        let executor = NullExecutor::new();
        let mut state = NavigatorState::default();
        state
            .import_asset_paths
            .extend(import_asset_paths(&movie_url, movie_data)?);
        let state = Rc::new(RefCell::new(state));
        let navigator = ScaleformNavigator {
            movie_url: movie_url.clone(),
            provider,
            spawner: executor.spawner(),
            state: state.clone(),
        };
        Ok((navigator, Self { executor, state }, movie_url.to_string()))
    }

    pub(crate) fn run_until_stalled(&mut self) {
        self.executor.run();
    }

    /// Remove and return the resource loads recorded since the previous call.
    ///
    /// #2967 — this used to be `loads()`, a *clone* of a list nothing ever
    /// cleared: `NavigatorState.loads` grew for the whole life of the player,
    /// keyed by paths untrusted movie content chooses, and got more expensive
    /// to read (a full clone) the longer the menu stayed open. Draining here
    /// makes each raw fetch event a one-time observation the owner
    /// (`SwfPlayer::tick`) folds into its own deduplicated, capped view —
    /// the same split `take_errors`/`resource_errors` already has.
    pub(crate) fn take_loads(&mut self) -> Vec<ScaleformResourceLoad> {
        std::mem::take(&mut self.state.borrow_mut().loads)
    }

    /// Remove and return the fetch failures recorded since the previous call.
    ///
    /// #2720 — this used to be `first_error()`, a *peek* at a list nothing
    /// ever cleared. One failed dependency therefore answered `Some` forever,
    /// and the player's `tick` copied that into a latch whose first statement
    /// was an early return: a single missing file — including the entirely
    /// routine "not in this archive" case — froze the whole menu for the rest
    /// of the session. Draining makes each failure a one-time event the owner
    /// records and moves past.
    pub(crate) fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.state.borrow_mut().errors)
    }
}

pub(crate) struct ScaleformNavigator {
    movie_url: Url,
    provider: Arc<dyn ScaleformResourceProvider>,
    spawner: NullSpawner,
    state: Rc<RefCell<NavigatorState>>,
}

/// Record a fetch failure and answer with an empty placeholder movie.
///
/// Free function rather than a method so the deferred half of [`fetch`] —
/// which owns a cloned `state` handle and cannot name `self` — reports
/// failures through the exact same path as the eager half. See
/// [`ScaleformNavigator::fetch`].
///
/// #2720 — this used to build an `ErrorResponse`, and returning that `Err` is
/// what froze the menu, not only through the engine-side latch that fix
/// removes. Ruffle sets `MovieClip::preload_progress.awaiting_import` before
/// spawning an `ImportAssets` fetch and clears it **only** on
/// `LoadManager::load_asset_movie`'s success path; `MovieClip::preload`
/// returns `false` for as long as that flag is set, and the root timeline will
/// not advance past a frame it has not preloaded. So a failed dependency fetch
/// wedges the movie inside Ruffle, no matter how the caller handles the error.
///
/// Handing back a valid, empty SWF lets the import *complete* with no symbols:
/// whatever the menu imported is missing (Ruffle logs the unresolved
/// character), but the movie preloads, runs, and draws. The failure is not
/// swallowed — it is recorded for `SwfPlayer::resource_errors` and logged by
/// the owning player, which is what makes "this menu is missing an asset" a
/// diagnosis rather than a silent hang.
///
/// [`fetch`]: ScaleformNavigator::fetch
fn record_degraded(
    state: &Rc<RefCell<NavigatorState>>,
    url: &str,
    message: impl Into<String>,
) -> Box<dyn SuccessResponse> {
    let message = message.into();
    log::debug!("Scaleform archive fetch failed, substituting an empty movie: {message}");
    state.borrow_mut().errors.push(message);
    Box::new(MemoryResponse {
        url: url.to_string(),
        body: placeholder_movie(),
        chunk_sent: false,
    })
}

/// The deferred half of [`ScaleformNavigator::fetch`]: everything that costs
/// real time.
///
/// #2734 (CONC-D7-UI-06) — `fetch` returns an `OwnedFuture`, but every path
/// used to do its work *eagerly* and hand back an already-computed value
/// wrapped in `Box::pin(async move { Ok(..) })`. That work is not cheap: a
/// full `provider.load` archive extract (zlib/LZ4 inflate for BSA/BA2), and
/// for import assets a `decompress_swf` + `parse_swf` + tag-record rewrite.
/// Ruffle calls `fetch` from inside `player.preload()` / `player.tick()`, so
/// all of it landed as one main-loop stall with no opportunity for the local
/// executor to interleave — `MAX_ARCHIVE_PRELOAD_PASSES` bounds the pass
/// count but not the per-pass cost.
///
/// Moving it here makes the future actually lazy: `drive_archive_preload`
/// alternates `preload()` with `run_until_stalled()`, so the cost is now
/// amortised across pump passes instead of blocking one of them. Combined
/// with the `Arc` provider, this is also the shape a real worker handoff
/// needs — the body no longer closes over the navigator.
///
/// Infallible since #2720: every failure below answers with
/// [`record_degraded`]'s placeholder movie rather than an `Err`, so the
/// signature says so.
fn load_archive_resource(
    provider: &dyn ScaleformResourceProvider,
    state: &Rc<RefCell<NavigatorState>>,
    request_url: String,
    resolved: Url,
    archive_path: String,
) -> Box<dyn SuccessResponse> {
    let body = match provider.load(&archive_path) {
        Ok(Some(body)) => body,
        Ok(None) => {
            return record_degraded(
                state,
                &request_url,
                format!(
                    "Scaleform resource {archive_path:?} was not found in the configured archive"
                ),
            );
        }
        Err(source) => {
            return record_degraded(
                state,
                &request_url,
                format!("failed to extract Scaleform resource {archive_path:?}: {source}"),
            );
        }
    };
    let is_import_asset = state.borrow().import_asset_paths.contains(&archive_path);
    let (body, import_preload_rewritten) = if is_import_asset {
        match prepare_import_asset_swf(&body) {
            Ok(body) => body,
            Err(message) => return record_degraded(state, &request_url, message),
        }
    } else {
        (body, false)
    };
    if is_import_asset {
        match import_asset_paths(&resolved, &body) {
            Ok(paths) => state.borrow_mut().import_asset_paths.extend(paths),
            Err(message) => return record_degraded(state, &request_url, message),
        }
    }

    state.borrow_mut().loads.push(ScaleformResourceLoad {
        request_url,
        archive_path,
        byte_len: body.len(),
        import_preload_rewritten,
        hit_count: 1,
    });
    Box::new(MemoryResponse {
        url: resolved.to_string(),
        body,
        chunk_sent: false,
    })
}

impl ScaleformNavigator {
    fn degraded(&self, url: &str, message: impl Into<String>) -> Box<dyn SuccessResponse> {
        record_degraded(&self.state, url, message)
    }
}

/// A valid, empty, single-frame SWF — see [`record_degraded`].
///
/// Deliberately carries no `FileAttributes` tag: an imported movie with no
/// exports has nothing for either VM to run, and the AVM1 form is the one
/// Ruffle accepts from every importer.
fn placeholder_movie() -> Vec<u8> {
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 1;
    let mut bytes = Vec::new();
    swf::write_swf(&header, &[Tag::ShowFrame], &mut bytes)
        .expect("writing a fixed, empty in-memory SWF cannot fail");
    bytes
}

impl NavigatorBackend for ScaleformNavigator {
    fn navigate_to_url(
        &self,
        url: &str,
        target: &str,
        _vars_method: Option<(NavigationMethod, IndexMap<String, String>)>,
    ) {
        log::debug!("Scaleform navigation ignored: url={url:?}, target={target:?}");
    }

    fn fetch(&self, request: Request) -> OwnedFuture<Box<dyn SuccessResponse>, ErrorResponse> {
        let request_url = request.url().to_string();
        if request.method() != NavigationMethod::Get {
            let response = self.degraded(
                &request_url,
                format!("Scaleform archive fetch only supports GET: {request_url}"),
            );
            return Box::pin(async move { Ok(response) });
        }

        let resolved = match self.resolve_url(&request_url) {
            Ok(url) => url,
            Err(error) => {
                let response = self.degraded(
                    &request_url,
                    format!("invalid Scaleform resource URL {request_url:?}: {error}"),
                );
                return Box::pin(async move { Ok(response) });
            }
        };
        let archive_path = match archive_path_from_url(&resolved) {
            Ok(path) => path,
            Err(message) => {
                let response = self.degraded(&request_url, message);
                return Box::pin(async move { Ok(response) });
            }
        };
        // #2734 — everything above is pure string/URL validation: cheap, and
        // worth keeping eager so a malformed request is still recorded in
        // `errors` immediately rather than waiting for a pump pass. Everything
        // below touches the archive, so it moves into the future body.
        let provider = Arc::clone(&self.provider);
        let state = Rc::clone(&self.state);
        Box::pin(async move {
            Ok(load_archive_resource(
                provider.as_ref(),
                &state,
                request_url,
                resolved,
                archive_path,
            ))
        })
    }

    fn resolve_url(&self, url: &str) -> Result<Url, ParseError> {
        self.movie_url.join(&url.replace('\\', "/"))
    }

    fn spawn_future(&mut self, future: OwnedFuture<(), Error>) {
        self.spawner.spawn_local(future);
    }

    fn pre_process_url(&self, url: Url) -> Url {
        url
    }

    fn connect_socket(
        &mut self,
        _host: String,
        _port: u16,
        _timeout: Duration,
        handle: SocketHandle,
        _receiver: Receiver<Vec<u8>>,
        sender: Sender<SocketAction>,
    ) {
        let _ = sender.try_send(SocketAction::Connect(handle, ConnectionState::Failed));
    }
}

struct MemoryResponse {
    url: String,
    body: Vec<u8>,
    chunk_sent: bool,
}

impl SuccessResponse for MemoryResponse {
    fn url(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.url)
    }

    fn set_url(&mut self, url: String) {
        self.url = url;
    }

    fn body(self: Box<Self>) -> OwnedFuture<Vec<u8>, Error> {
        Box::pin(async move { Ok(self.body) })
    }

    fn text_encoding(&self) -> Option<&'static Encoding> {
        None
    }

    fn status(&self) -> u16 {
        200
    }

    fn redirected(&self) -> bool {
        false
    }

    fn next_chunk(&mut self) -> OwnedFuture<Option<Vec<u8>>, Error> {
        let chunk = if self.chunk_sent {
            None
        } else {
            self.chunk_sent = true;
            Some(self.body.clone())
        };
        Box::pin(async move { Ok(chunk) })
    }

    fn expected_length(&self) -> Result<Option<u64>, Error> {
        Ok(Some(self.body.len() as u64))
    }
}

fn archive_movie_url(movie_path: &str) -> Result<Url, String> {
    let mut virtual_path = PathBuf::from("/");
    for component in movie_path.replace('\\', "/").split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err(format!(
                    "Scaleform movie path may not escape the archive root: {movie_path:?}"
                ));
            }
            component => virtual_path.push(component),
        }
    }
    if virtual_path == Path::new("/") {
        return Err("Scaleform movie path is empty".to_string());
    }
    Url::from_file_path(&virtual_path)
        .map_err(|()| format!("invalid Scaleform movie path: {movie_path:?}"))
}

fn archive_path_from_url(url: &Url) -> Result<String, String> {
    if url.scheme() != "file" {
        return Err(format!(
            "Scaleform archive navigator refuses non-local URL: {url}"
        ));
    }
    let mut file_url = url.clone();
    file_url.set_query(None);
    file_url.set_fragment(None);
    let path = file_url
        .to_file_path()
        .map_err(|()| format!("could not convert Scaleform URL to an archive path: {url}"))?;
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(component) => {
                components.push(component.to_string_lossy().into_owned());
            }
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => {
                return Err(format!(
                    "unsafe Scaleform archive path resolved from URL: {url}"
                ));
            }
        }
    }
    if components.is_empty() {
        return Err(format!(
            "empty Scaleform archive path resolved from URL: {url}"
        ));
    }
    Ok(components.join("\\"))
}

fn import_asset_paths(movie_url: &Url, movie_data: &[u8]) -> Result<Vec<String>, String> {
    if !is_swf(movie_data) {
        return Ok(Vec::new());
    }
    let decompressed = swf::decompress_swf(movie_data)
        .map_err(|error| format!("failed to decompress imported Scaleform movie: {error}"))?;
    let movie = swf::parse_swf(&decompressed)
        .map_err(|error| format!("failed to parse imported Scaleform movie: {error}"))?;
    movie
        .tags
        .iter()
        .filter_map(|tag| match tag {
            Tag::ImportAssets { url, .. } => Some(url.to_string_lossy(swf::UTF_8)),
            _ => None,
        })
        .map(|relative| {
            movie_url
                .join(&relative.replace('\\', "/"))
                .map_err(|error| {
                    format!("invalid ImportAssets URL {relative:?} in {movie_url}: {error}")
                })
                .and_then(|url| archive_path_from_url(&url))
        })
        .collect()
}

fn prepare_import_asset_swf(movie_data: &[u8]) -> Result<(Vec<u8>, bool), String> {
    if !is_swf(movie_data) {
        return Ok((movie_data.to_vec(), false));
    }
    let decompressed = swf::decompress_swf(movie_data)
        .map_err(|error| format!("failed to decompress imported Scaleform movie: {error}"))?;
    if !decompressed.header.is_action_script_3() {
        return Ok((movie_data.to_vec(), false));
    }

    let records = raw_tag_records(&decompressed.data)?;
    let risky_tag = records.iter().find_map(|(code, start)| {
        matches!(
            *code,
            code if code == TagCode::DoAbc as u16
                || code == TagCode::DoAbc2 as u16
                || code == TagCode::SymbolClass as u16
        )
        .then_some(*start)
    });
    let first_frame = records
        .iter()
        .find_map(|(code, start)| (*code == TagCode::ShowFrame as u16).then_some(*start));
    let Some(risky_tag) = risky_tag else {
        return Ok((movie_data.to_vec(), false));
    };
    if first_frame.is_some_and(|first_frame| first_frame < risky_tag) {
        return Ok((movie_data.to_vec(), false));
    }

    // Ruffle's ImportAssets path currently calls set_cur_preload_frame(0),
    // while its AVM2 bytecode/symbol preload path indexes `frame - 1`.
    // A synthetic boundary immediately before the first affected tag restores
    // the normal root-movie starting state: the original tag is still
    // associated with frame zero, and definitions/exports remain unchanged.
    let mut tags = decompressed.data;
    tags.splice(risky_tag..risky_tag, [0x40, 0x00]);
    let mut rewritten = Vec::new();
    swf::write::write_swf_raw_tags(decompressed.header.swf_header(), &tags, &mut rewritten)
        .map_err(|error| format!("failed to rewrite imported Scaleform movie: {error}"))?;
    Ok((rewritten, true))
}

fn is_swf(data: &[u8]) -> bool {
    matches!(data.get(..3), Some(b"FWS" | b"CWS" | b"ZWS"))
}

fn raw_tag_records(data: &[u8]) -> Result<Vec<(u16, usize)>, String> {
    let mut records = Vec::new();
    let mut cursor = 0;
    while cursor < data.len() {
        let start = cursor;
        let header = data
            .get(cursor..cursor + 2)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u16::from_le_bytes)
            .ok_or_else(|| "truncated SWF tag header in imported movie".to_string())?;
        cursor += 2;
        let code = header >> 6;
        let short_len = (header & 0x3f) as usize;
        let len = if short_len == 0x3f {
            let len = data
                .get(cursor..cursor + 4)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u32::from_le_bytes)
                .ok_or_else(|| "truncated long SWF tag header in imported movie".to_string())?
                as usize;
            cursor += 4;
            len
        } else {
            short_len
        };
        cursor = cursor
            .checked_add(len)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| format!("SWF tag {code} exceeds imported movie bounds"))?;
        records.push((code, start));
        if code == TagCode::End as u16 {
            break;
        }
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io;
    use std::sync::Arc;

    use futures::executor::block_on;
    use ruffle_core::backend::navigator::{NavigatorBackend, Request};
    use swf::{FileAttributes, SwfStr, Tag};

    use super::{
        archive_movie_url, archive_path_from_url, placeholder_movie, ScaleformNavigatorRuntime,
        ScaleformResourceProvider,
    };
    use crate::{ScaleformProfile, SwfPlayer};

    struct MemoryProvider(HashMap<String, Vec<u8>>);

    impl ScaleformResourceProvider for MemoryProvider {
        fn load(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
            Ok(self.0.get(path).cloned())
        }
    }

    /// Counts `load` calls so a test can tell "the future was constructed"
    /// from "the archive was actually read".
    struct CountingProvider {
        inner: MemoryProvider,
        loads: std::sync::atomic::AtomicUsize,
    }

    impl CountingProvider {
        fn count(&self) -> usize {
            self.loads.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl ScaleformResourceProvider for CountingProvider {
        fn load(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
            self.loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.load(path)
        }
    }

    /// Regression for #2734 (CONC-D7-UI-06) — `fetch` returns an
    /// `OwnedFuture`, so it must not have done the work by the time it
    /// returns. Pre-fix every path ran `provider.load` (a full zlib/LZ4
    /// archive inflate on real data) plus, for import assets, a
    /// `decompress_swf` + `parse_swf` + tag rewrite, then wrapped the
    /// finished value in `Box::pin(async move { Ok(..) })`. Ruffle calls
    /// `fetch` from inside `preload()`/`tick()`, so that landed as a
    /// main-loop stall the local executor had no chance to interleave.
    ///
    /// Holding the future unpolled is the whole test: a provider whose
    /// `load` has not been called yet proves the work is deferred, and
    /// driving it afterwards proves nothing was lost by deferring.
    #[test]
    fn fetch_defers_archive_io_until_the_future_is_polled() {
        let provider = Arc::new(CountingProvider {
            inner: MemoryProvider(HashMap::from([(
                "interface\\fonts_en.swf".to_string(),
                b"font movie".to_vec(),
            )])),
            loads: std::sync::atomic::AtomicUsize::new(0),
        });
        let (navigator, mut runtime, _) = ScaleformNavigatorRuntime::create(
            "interface\\hudmenu.swf",
            b"",
            provider.clone() as Arc<dyn ScaleformResourceProvider>,
        )
        .unwrap();

        let pending = navigator.fetch(Request::get("fonts_en.swf".to_string()));
        assert_eq!(
            provider.count(),
            0,
            "constructing the fetch future must not touch the archive — that \
             is the entire point of returning a future"
        );
        assert!(
            runtime.take_loads().is_empty(),
            "no load may be recorded before the future runs"
        );

        let response = match block_on(pending) {
            Ok(response) => response,
            Err(error) => panic!("deferred fetch should succeed: {}", error.error),
        };
        assert_eq!(provider.count(), 1, "polling must perform exactly one load");
        assert_eq!(block_on(response.body()).unwrap(), b"font movie");
        assert_eq!(
            runtime.take_loads()[0].archive_path,
            "interface\\fonts_en.swf"
        );
    }

    /// The cheap half stays eager on purpose: a malformed URL is pure string
    /// validation, so it is recorded immediately without waiting for a pump
    /// pass — and it must never reach the archive.
    ///
    /// The *answer* is a placeholder movie rather than an `Err` since #2720:
    /// Ruffle's `awaiting_import` flag only clears on a successful fetch, so
    /// returning `Err` here wedges the importing movie's preload. The failure
    /// still lands in `errors`, which is what the player reports.
    #[test]
    fn malformed_request_is_recorded_eagerly_without_touching_the_archive() {
        let provider = Arc::new(CountingProvider {
            inner: MemoryProvider(HashMap::new()),
            loads: std::sync::atomic::AtomicUsize::new(0),
        });
        let (navigator, mut runtime, _) = ScaleformNavigatorRuntime::create(
            "interface\\hudmenu.swf",
            b"",
            provider.clone() as Arc<dyn ScaleformResourceProvider>,
        )
        .unwrap();

        // Non-local scheme — rejected by `archive_path_from_url`.
        let pending = navigator.fetch(Request::get("http://example.com/x.swf".to_string()));
        let errors = runtime.take_errors();
        assert_eq!(
            errors.len(),
            1,
            "URL validation stays eager, so the failure is visible immediately: {errors:?}"
        );
        let Ok(response) = block_on(pending) else {
            panic!("a rejected URL degrades to a placeholder, it does not Err");
        };
        assert_eq!(
            block_on(response.body()).unwrap(),
            placeholder_movie(),
            "the answer must be the empty placeholder movie"
        );
        assert_eq!(
            provider.count(),
            0,
            "a rejected URL must not hit the archive"
        );
    }

    #[test]
    fn relative_urls_resolve_to_the_movie_archive_directory() {
        let provider = Arc::new(MemoryProvider(HashMap::from([(
            "interface\\fonts_en.swf".to_string(),
            b"font movie".to_vec(),
        )])));
        let (navigator, mut runtime, movie_url) =
            ScaleformNavigatorRuntime::create("interface\\hudmenu.swf", b"", provider).unwrap();

        assert_eq!(movie_url, "file:///interface/hudmenu.swf");
        let response = match block_on(navigator.fetch(Request::get("fonts_en.swf".to_string()))) {
            Ok(response) => response,
            Err(error) => panic!("archive fetch failed: {}", error.error),
        };
        assert_eq!(block_on(response.body()).unwrap(), b"font movie");
        assert_eq!(
            runtime.take_loads()[0].archive_path,
            "interface\\fonts_en.swf"
        );
    }

    #[test]
    fn archive_paths_are_confined_and_percent_decoded() {
        let url = archive_movie_url("interface\\shared menus\\hud.swf").unwrap();
        assert_eq!(
            archive_path_from_url(&url).unwrap(),
            "interface\\shared menus\\hud.swf"
        );
        assert!(archive_movie_url("interface\\..\\outside.swf").is_err());
    }

    #[test]
    fn player_preloads_relative_imports_before_advancing_frame_one() {
        let imported = movie(vec![
            Tag::FileAttributes(FileAttributes::IS_ACTION_SCRIPT_3),
            Tag::DoAbc(&[0, 0, 0, 0]),
        ]);
        let root = movie(vec![
            Tag::FileAttributes(FileAttributes::IS_ACTION_SCRIPT_3),
            Tag::ImportAssets {
                url: SwfStr::from_utf8_str("fonts_en.swf"),
                imports: Vec::new(),
            },
        ]);
        let provider = Arc::new(MemoryProvider(HashMap::from([
            ("interface\\hudmenu.swf".to_string(), root),
            ("interface\\fonts_en.swf".to_string(), imported),
        ])));

        let mut player = SwfPlayer::from_resource_provider(
            provider,
            "interface\\hudmenu.swf",
            64,
            64,
            ScaleformProfile::Fallout4Avm2,
        )
        .unwrap();
        assert_eq!(
            player.resource_loads()[0].archive_path,
            "interface\\fonts_en.swf"
        );
        assert!(player.resource_loads()[0].import_preload_rewritten);
        player.tick(1.0 / 30.0);
        assert_eq!(player.current_frame(), Some(1));
        assert_eq!(player.resource_error(), None);
    }

    /// Regression for #2720 / CONC-D7-UI-04: a dependency that isn't in the
    /// configured archive must not stop the menu.
    ///
    /// `ScaleformNavigator::fail` pushed onto a `NavigatorState::errors` list
    /// nothing ever cleared, `first_error()` peeked at it, and `tick`'s first
    /// statement returned early whenever that peek was `Some`. So **one**
    /// missing file — including the entirely routine `Ok(None)` "not in this
    /// archive" case, on a navigator that holds exactly one archive — froze
    /// the whole movie for the rest of the session, leaving the last uploaded
    /// frame on screen. The failure has to be recorded and reported, not
    /// latched.
    #[test]
    fn a_missing_dependency_is_recorded_without_freezing_the_movie() {
        let root = movie(vec![
            Tag::FileAttributes(FileAttributes::IS_ACTION_SCRIPT_3),
            Tag::ImportAssets {
                url: SwfStr::from_utf8_str("fonts_en.swf"),
                imports: Vec::new(),
            },
        ]);
        // Deliberately NOT supplying interface\fonts_en.swf.
        let provider = Arc::new(MemoryProvider(HashMap::from([(
            "interface\\hudmenu.swf".to_string(),
            root,
        )])));

        let mut player = SwfPlayer::from_resource_provider(
            provider,
            "interface\\hudmenu.swf",
            64,
            64,
            ScaleformProfile::Fallout4Avm2,
        )
        .expect("a missing dependency must not fail the load of a root movie that parsed");

        let error = player
            .resource_error()
            .expect("the failed fetch must still be reported");
        assert!(
            error.contains("fonts_en.swf"),
            "the report must name the missing file: {error}"
        );
        assert_eq!(
            player.resource_errors().len(),
            1,
            "one missing file, one recorded failure: {:?}",
            player.resource_errors()
        );
        // The placeholder answer is what lets Ruffle's own `awaiting_import`
        // flag clear (`LoadManager::load_asset_movie` only calls
        // `finish_importing()` on its success path), so the preload settles
        // rather than wedging the root timeline behind an unpreloaded frame.
        assert!(
            !player.preload_stalled(),
            "the placeholder import must let the preload settle"
        );

        // And the movie runs: the latch is what's gone, not the player.
        for _ in 0..3 {
            player.tick(1.0 / 30.0);
        }
        assert_eq!(player.current_frame(), Some(1));
        assert_eq!(
            player.resource_errors().len(),
            1,
            "a repeated failure must dedup rather than accumulate: {:?}",
            player.resource_errors()
        );
    }

    /// Regression for #2719 / CONC-D7-UI-03: `tick` ended with an
    /// unconditional `self.dirty = true`, so `render`'s `if !self.dirty`
    /// early exit was dead code and a *static* menu re-rendered, re-read back
    /// and re-uploaded a full-viewport RGBA image every single frame. Each of
    /// those uploads builds a fresh `VkImage` and blocks on a one-time
    /// submit's fence ahead of `draw_frame`. A movie whose picture is not
    /// changing must hand the caller pixels once and then stop.
    #[test]
    fn a_static_movie_stops_handing_back_pixels_after_the_first_frame() {
        let provider = Arc::new(MemoryProvider(HashMap::from([(
            "interface\\hudmenu.swf".to_string(),
            movie(vec![Tag::FileAttributes(
                FileAttributes::IS_ACTION_SCRIPT_3,
            )]),
        )])));
        let mut player = SwfPlayer::from_resource_provider(
            provider,
            "interface\\hudmenu.swf",
            64,
            64,
            ScaleformProfile::Fallout4Avm2,
        )
        .unwrap();

        player.tick(1.0 / 30.0);
        assert!(
            player.render().is_some(),
            "the first frame must always be uploaded — the caller has no texture yet"
        );

        for frame in 0..8 {
            player.tick(1.0 / 30.0);
            assert!(
                player.render().is_none(),
                "frame {frame} of a static movie re-uploaded an unchanged image"
            );
        }
    }

    fn movie(mut tags: Vec<Tag<'_>>) -> Vec<u8> {
        let mut header = swf::Header::default_with_swf_version(15);
        header.num_frames = 1;
        header.frame_rate = swf::Fixed8::from_f32(30.0);
        tags.push(Tag::ShowFrame);
        let mut bytes = Vec::new();
        swf::write_swf(&header, &tags, &mut bytes).unwrap();
        bytes
    }
}
