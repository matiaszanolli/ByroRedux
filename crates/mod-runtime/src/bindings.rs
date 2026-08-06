//! Generated Component Model bindings for the stable guest/host boundary.

wasmtime::component::bindgen!({
    world: "extension",
    path: "wit",
    imports: { default: trappable },
});
