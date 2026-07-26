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
pub trait ScaleformResourceProvider {
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaleformResourceLoad {
    pub request_url: String,
    pub archive_path: String,
    pub byte_len: usize,
    /// Ruffle currently starts `ImportAssets` children at preload frame zero.
    /// This is true when a leading frame boundary was synthesized to keep
    /// AVM2 bytecode and symbol tags associated with their intended frame.
    pub import_preload_rewritten: bool,
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
        provider: Rc<dyn ScaleformResourceProvider>,
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

    pub(crate) fn loads(&self) -> Vec<ScaleformResourceLoad> {
        self.state.borrow().loads.clone()
    }

    pub(crate) fn first_error(&self) -> Option<String> {
        self.state.borrow().errors.first().cloned()
    }
}

pub(crate) struct ScaleformNavigator {
    movie_url: Url,
    provider: Rc<dyn ScaleformResourceProvider>,
    spawner: NullSpawner,
    state: Rc<RefCell<NavigatorState>>,
}

impl ScaleformNavigator {
    fn fail(&self, url: &str, message: impl Into<String>) -> ErrorResponse {
        let message = message.into();
        self.state.borrow_mut().errors.push(message.clone());
        ErrorResponse {
            url: url.to_string(),
            error: Error::FetchError(message),
        }
    }
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
            let error = self.fail(
                &request_url,
                format!("Scaleform archive fetch only supports GET: {request_url}"),
            );
            return Box::pin(async move { Err(error) });
        }

        let resolved = match self.resolve_url(&request_url) {
            Ok(url) => url,
            Err(error) => {
                let error = self.fail(
                    &request_url,
                    format!("invalid Scaleform resource URL {request_url:?}: {error}"),
                );
                return Box::pin(async move { Err(error) });
            }
        };
        let archive_path = match archive_path_from_url(&resolved) {
            Ok(path) => path,
            Err(message) => {
                let error = self.fail(&request_url, message);
                return Box::pin(async move { Err(error) });
            }
        };
        let body = match self.provider.load(&archive_path) {
            Ok(Some(body)) => body,
            Ok(None) => {
                let error = self.fail(
                    &request_url,
                    format!(
                        "Scaleform resource {archive_path:?} was not found in the configured archive"
                    ),
                );
                return Box::pin(async move { Err(error) });
            }
            Err(source) => {
                let error = self.fail(
                    &request_url,
                    format!("failed to extract Scaleform resource {archive_path:?}: {source}"),
                );
                return Box::pin(async move { Err(error) });
            }
        };
        let is_import_asset = self
            .state
            .borrow()
            .import_asset_paths
            .contains(&archive_path);
        let (body, import_preload_rewritten) = if is_import_asset {
            match prepare_import_asset_swf(&body) {
                Ok(body) => body,
                Err(message) => {
                    let error = self.fail(&request_url, message);
                    return Box::pin(async move { Err(error) });
                }
            }
        } else {
            (body, false)
        };
        if is_import_asset {
            match import_asset_paths(&resolved, &body) {
                Ok(paths) => self.state.borrow_mut().import_asset_paths.extend(paths),
                Err(message) => {
                    let error = self.fail(&request_url, message);
                    return Box::pin(async move { Err(error) });
                }
            }
        }

        self.state.borrow_mut().loads.push(ScaleformResourceLoad {
            request_url,
            archive_path,
            byte_len: body.len(),
            import_preload_rewritten,
        });
        let response: Box<dyn SuccessResponse> = Box::new(MemoryResponse {
            url: resolved.to_string(),
            body,
            chunk_sent: false,
        });
        Box::pin(async move { Ok(response) })
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
    use std::rc::Rc;

    use futures::executor::block_on;
    use ruffle_core::backend::navigator::{NavigatorBackend, Request};
    use swf::{FileAttributes, SwfStr, Tag};

    use super::{
        archive_movie_url, archive_path_from_url, ScaleformNavigatorRuntime,
        ScaleformResourceProvider,
    };
    use crate::{ScaleformProfile, SwfPlayer};

    struct MemoryProvider(HashMap<String, Vec<u8>>);

    impl ScaleformResourceProvider for MemoryProvider {
        fn load(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
            Ok(self.0.get(path).cloned())
        }
    }

    #[test]
    fn relative_urls_resolve_to_the_movie_archive_directory() {
        let provider = Rc::new(MemoryProvider(HashMap::from([(
            "interface\\fonts_en.swf".to_string(),
            b"font movie".to_vec(),
        )])));
        let (navigator, runtime, movie_url) =
            ScaleformNavigatorRuntime::create("interface\\hudmenu.swf", b"", provider).unwrap();

        assert_eq!(movie_url, "file:///interface/hudmenu.swf");
        let response = match block_on(navigator.fetch(Request::get("fonts_en.swf".to_string()))) {
            Ok(response) => response,
            Err(error) => panic!("archive fetch failed: {}", error.error),
        };
        assert_eq!(block_on(response.body()).unwrap(), b"font movie");
        assert_eq!(runtime.loads()[0].archive_path, "interface\\fonts_en.swf");
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
        let provider = Rc::new(MemoryProvider(HashMap::from([
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
