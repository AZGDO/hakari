pub mod renderer;

// Re-export the main render function and a few helpers to preserve the old API
pub use renderer::render;

// Keep specific helper exports used elsewhere
pub use renderer::get_input_height;
pub use renderer::render_status_bar;
// render_welcome_box is internal now; callers should use messages::render_welcome_screen
