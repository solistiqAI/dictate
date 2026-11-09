//! The surfaces of the overlay and the bar: Liquid Glass (`NSGlassEffectView`) from macOS 26,
//! the dark HUD material before. The glass is dimmed beneath its content, so white text reads
//! over a bright page too (measured over white: grey 112 → 86, contrast with white about 7:1; a
//! dark `tintColor` made it lighter instead). Main thread only.

use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSColor, NSGlassEffectView, NSGlassEffectViewStyle, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// A rounded surface: place and show `view`, put controls into `content`.
pub struct Hud {
    pub view: Retained<NSView>,
    pub content: Retained<NSView>,
    glass: Option<Retained<NSGlassEffectView>>,
}

/// Black over the glass, beneath the content.
const DIM: f64 = 0.3;

static NO_GLASS: AtomicBool = AtomicBool::new(false);

/// The HUD material from now on, also on macOS 26: for the PNG previews, as the glass and
/// everything in it is composited on screen only (`cacheDisplayInRect` gets an empty image).
pub fn without_glass() {
    NO_GLASS.store(true, Ordering::Relaxed);
}

/// Whether to draw Liquid Glass: macOS 26 and later, not for a preview.
fn has_glass() -> bool {
    !NO_GLASS.load(Ordering::Relaxed) && AnyClass::get(c"NSGlassEffectView").is_some()
}

impl Hud {
    /// `border`: a thin light edge on the old material (the glass draws its own).
    pub fn new(mtm: MainThreadMarker, radius: f64, border: bool) -> Self {
        if has_glass() {
            let glass = NSGlassEffectView::new(mtm);
            glass.setStyle(NSGlassEffectViewStyle::Regular);
            glass.setCornerRadius(radius);
            let content = NSView::initWithFrame(
                NSView::alloc(mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
            );
            content.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            content.setWantsLayer(true);
            if let Some(layer) = content.layer() {
                layer.setBackgroundColor(Some(&NSColor::colorWithWhite_alpha(0.0, DIM).CGColor()));
                layer.setCornerRadius(radius);
            }
            glass.setContentView(Some(&content));
            let view = Retained::into_super(glass.clone());
            return Hud {
                view,
                content,
                glass: Some(glass),
            };
        }
        let effect = NSVisualEffectView::new(mtm);
        effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
        effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect.setState(NSVisualEffectState::Active);
        effect.setWantsLayer(true);
        if let Some(layer) = effect.layer() {
            layer.setCornerRadius(radius);
            layer.setMasksToBounds(true);
            if border {
                layer.setBorderWidth(0.5);
                layer.setBorderColor(Some(&NSColor::colorWithWhite_alpha(1.0, 0.18).CGColor()));
            }
        }
        let view = Retained::into_super(effect);
        Hud {
            content: view.clone(),
            view,
            glass: None,
        }
    }

    pub fn set_radius(&self, radius: f64) {
        match &self.glass {
            Some(glass) => {
                glass.setCornerRadius(radius);
                if let Some(layer) = self.content.layer() {
                    layer.setCornerRadius(radius);
                }
            }
            None => {
                if let Some(layer) = self.view.layer() {
                    layer.setCornerRadius(radius);
                }
            }
        }
    }

    pub fn set_frame(&self, frame: NSRect) {
        self.view.setFrame(frame);
        // The glass sizes its content view itself; set it too in case it waits for a layout pass.
        if self.glass.is_some() {
            self.content
                .setFrame(NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
        }
    }
}
