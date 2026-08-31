//! Adapts nice-plug-egui's pixel-based editor contract to macOS plug-in hosts.
//!
//! AppKit embeds plug-in views and reports their frames in points. Baseview also
//! exposes the Retina backing scale, but passing that scale through NICE-PLUG's
//! host resize callbacks makes the VST3/CLAP parent twice as large as the NSView.

use nice_plug::context::gui::GuiContext;
#[cfg(target_os = "macos")]
use nice_plug::editor::HostCallbacks;
use nice_plug::editor::dpi::PhysicalSize;
#[cfg(target_os = "macos")]
use nice_plug::editor::dpi::{LogicalSize, Size};
use nice_plug::editor::{
    Editor, EditorHandle, HostMethods, Modifiers, ParentWindowHandle, ResizeHint, SpawnedEditor,
    VirtualKeyCode,
};
use nice_plug_egui::baseview;
use nice_plug_egui::{EguiEditor, EguiEditorHandle, EguiEditorState, NiceEguiApp};
use std::error::Error;
use std::sync::Arc;

pub struct HostCoordinateEditor<A: NiceEguiApp> {
    inner: EguiEditor<A>,
    state: Arc<EguiEditorState>,
}

impl<A: NiceEguiApp> HostCoordinateEditor<A> {
    pub(crate) fn new(inner: EguiEditor<A>, state: Arc<EguiEditorState>) -> Self {
        Self { inner, state }
    }

    fn nominal_size(&self) -> PhysicalSize<u32> {
        #[cfg(target_os = "macos")]
        {
            let size: LogicalSize<f64> = self.state.size().to_logical(1.0);
            PhysicalSize {
                width: size.width.round().max(1.0) as u32,
                height: size.height.round().max(1.0) as u32,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Editor::size(&self.inner)
        }
    }
}

#[cfg(target_os = "macos")]
struct MacosHostCallbacks {
    inner: Box<dyn HostCallbacks>,
}

#[cfg(target_os = "macos")]
impl HostCallbacks for MacosHostCallbacks {
    fn request_resize(&mut self, new_size: Size, scale_factor: f64) -> Result<(), Box<dyn Error>> {
        let logical = new_size.to_logical(scale_factor);
        self.inner.request_resize(
            Size::Logical(LogicalSize {
                width: logical.width,
                height: logical.height,
            }),
            1.0,
        )
    }

    fn destroyed(&mut self) {
        self.inner.destroyed();
    }
}

impl<A: NiceEguiApp> Editor for HostCoordinateEditor<A> {
    type Handle = HostCoordinateHandle;

    fn spawn(
        &self,
        parent: Option<ParentWindowHandle>,
        wait_for_parent: bool,
        fallback_scale_factor: Option<f64>,
        gui_context: GuiContext,
        host: Option<HostMethods>,
    ) -> Result<SpawnedEditor<Self::Handle>, Box<dyn Error>> {
        #[cfg(target_os = "macos")]
        let host = host.map(|host| HostMethods {
            callbacks: Box::new(MacosHostCallbacks {
                inner: host.callbacks,
            }),
            main_thread_caller: host.main_thread_caller,
        });

        let resize_hint = self.inner.resize_hint();
        let spawned = self.inner.spawn(
            parent,
            wait_for_parent,
            fallback_scale_factor,
            gui_context,
            host,
        )?;
        Ok(SpawnedEditor {
            handle: HostCoordinateHandle {
                inner: spawned.handle,
                resize_hint,
            },
            window: spawned.window,
        })
    }

    fn size(&self) -> PhysicalSize<u32> {
        self.nominal_size()
    }

    fn resize_hint(&self) -> ResizeHint {
        self.inner.resize_hint()
    }
}

pub struct HostCoordinateHandle {
    inner: EguiEditorHandle,
    resize_hint: ResizeHint,
}

impl EditorHandle for HostCoordinateHandle {
    type Window = baseview::Window;
    type Error = baseview::Error;

    fn run_until_closed(window: Self::Window) -> Result<(), Self::Error> {
        EguiEditorHandle::run_until_closed(window)
    }

    fn set_parent(
        &self,
        parent: ParentWindowHandle,
        window: &Self::Window,
    ) -> Result<(), Self::Error> {
        self.inner.set_parent(parent, window)
    }

    fn show(&self, window: &Self::Window) -> Result<(), Self::Error> {
        self.inner.show(window)
    }

    fn hide(&self, window: &Self::Window) -> Result<(), Self::Error> {
        self.inner.hide(window)
    }

    fn set_size(
        &self,
        new_size: PhysicalSize<u32>,
        window: &Self::Window,
    ) -> Result<(), Self::Error> {
        #[cfg(target_os = "macos")]
        {
            window.resize(baseview::dpi::LogicalSize::new(
                new_size.width as f64,
                new_size.height as f64,
            ))
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.inner.set_size(new_size, window)
        }
    }

    fn host_main_thread_callback(&self, window: &Self::Window) {
        self.inner.host_main_thread_callback(window);
    }

    fn adjust_size(
        &self,
        new_size: PhysicalSize<u32>,
        window: &Self::Window,
    ) -> Option<PhysicalSize<u32>> {
        #[cfg(target_os = "macos")]
        {
            let current = window.size().logical;
            let current = PhysicalSize {
                width: current.width.round().max(1.0) as u32,
                height: current.height.round().max(1.0) as u32,
            };
            Some(self.resize_hint.adjust_size(new_size, current, 1.0))
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.inner.adjust_size(new_size, window)
        }
    }

    fn set_fallback_scale_factor(
        &self,
        scale_factor: f64,
        window: &Self::Window,
    ) -> Result<(), Self::Error> {
        self.inner.set_fallback_scale_factor(scale_factor, window)
    }

    fn on_virtual_key_from_host(
        &self,
        key_code: VirtualKeyCode,
        is_down: bool,
        modifiers: Modifiers,
    ) -> bool {
        self.inner
            .on_virtual_key_from_host(key_code, is_down, modifiers)
    }

    fn state_changed(&self) {
        self.inner.state_changed();
    }

    fn param_value_changed(&self, id: &str, normalized_value: f32) {
        self.inner.param_value_changed(id, normalized_value);
    }

    fn param_modulation_changed(&self, id: &str, modulation_offset: f32) {
        self.inner.param_modulation_changed(id, modulation_offset);
    }
}
