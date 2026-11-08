//! Visual design: colors, fonts and the egui style.
//!
//! Everything about how the app looks lives here and in `kit.rs`, so a rebrand
//! (colors, fonts, name) is a change to a handful of constants.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    Theme, Vec2, Visuals, epaint::Shadow,
};
use std::sync::Arc;

/// Product name shown in the sidebar and window title.
pub const APP_NAME: &str = "Wedding Planner";

/// The colors used by every screen. Two instances exist: light and dark.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// App background.
    pub bg: Color32,
    /// Cards and popups.
    pub surface: Color32,
    /// Inputs, hover fills, subtle panels.
    pub surface_alt: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    /// A gentle tint of the accent, for selected chips and highlights.
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub sidebar_bg: Color32,
    pub sidebar_text: Color32,
    pub sidebar_muted: Color32,
    pub sidebar_hover: Color32,
    pub sidebar_active: Color32,
}

impl Palette {
    pub fn light() -> Self {
        Palette {
            dark: false,
            bg: Color32::from_rgb(0xFA, 0xF7, 0xF3),
            surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            surface_alt: Color32::from_rgb(0xF4, 0xEF, 0xE9),
            border: Color32::from_rgb(0xE9, 0xE2, 0xD9),
            text: Color32::from_rgb(0x2A, 0x26, 0x30),
            muted: Color32::from_rgb(0x7B, 0x73, 0x80),
            faint: Color32::from_rgb(0xAA, 0xA2, 0xAE),
            accent: Color32::from_rgb(0xB5, 0x53, 0x6F),
            accent_hover: Color32::from_rgb(0xA0, 0x44, 0x60),
            accent_soft: Color32::from_rgb(0xF7, 0xE6, 0xEB),
            on_accent: Color32::WHITE,
            success: Color32::from_rgb(0x4F, 0x8F, 0x6B),
            warning: Color32::from_rgb(0xC4, 0x84, 0x22),
            danger: Color32::from_rgb(0xC2, 0x4D, 0x4B),
            sidebar_bg: Color32::from_rgb(0x2B, 0x24, 0x31),
            sidebar_text: Color32::from_rgb(0xF1, 0xEA, 0xF3),
            sidebar_muted: Color32::from_rgb(0xA3, 0x98, 0xAB),
            sidebar_hover: Color32::from_rgb(0x38, 0x30, 0x40),
            sidebar_active: Color32::from_rgb(0x47, 0x3C, 0x51),
        }
    }

    pub fn dark() -> Self {
        Palette {
            dark: true,
            bg: Color32::from_rgb(0x17, 0x14, 0x1B),
            surface: Color32::from_rgb(0x20, 0x1C, 0x26),
            surface_alt: Color32::from_rgb(0x2A, 0x25, 0x31),
            border: Color32::from_rgb(0x38, 0x32, 0x40),
            text: Color32::from_rgb(0xF0, 0xEB, 0xF3),
            muted: Color32::from_rgb(0xA9, 0xA0, 0xB0),
            faint: Color32::from_rgb(0x78, 0x70, 0x80),
            accent: Color32::from_rgb(0xD9, 0x72, 0x92),
            accent_hover: Color32::from_rgb(0xE5, 0x8A, 0xA6),
            accent_soft: Color32::from_rgb(0x3C, 0x27, 0x30),
            on_accent: Color32::from_rgb(0x1E, 0x12, 0x17),
            success: Color32::from_rgb(0x6F, 0xB0, 0x8B),
            warning: Color32::from_rgb(0xDB, 0xA0, 0x4A),
            danger: Color32::from_rgb(0xE0, 0x70, 0x6D),
            sidebar_bg: Color32::from_rgb(0x10, 0x0E, 0x13),
            sidebar_text: Color32::from_rgb(0xF0, 0xEB, 0xF3),
            sidebar_muted: Color32::from_rgb(0x8C, 0x84, 0x94),
            sidebar_hover: Color32::from_rgb(0x1B, 0x17, 0x21),
            sidebar_active: Color32::from_rgb(0x2A, 0x24, 0x32),
        }
    }

    pub fn of(ctx: &egui::Context) -> Self {
        match ctx.theme() {
            Theme::Dark => Self::dark(),
            Theme::Light => Self::light(),
        }
    }
}

/// Colors a section can be tagged with (sidebar dots, calendar chips).
pub const SECTION_COLORS: [Color32; 8] = [
    Color32::from_rgb(0xD0, 0x67, 0x8A), // rose
    Color32::from_rgb(0x6F, 0xA2, 0x87), // sage
    Color32::from_rgb(0xD1, 0xA0, 0x4F), // gold
    Color32::from_rgb(0x5B, 0x9B, 0xC7), // sky
    Color32::from_rgb(0x9A, 0x82, 0xC9), // lavender
    Color32::from_rgb(0xE2, 0x7D, 0x60), // coral
    Color32::from_rgb(0x4F, 0xA8, 0xA5), // teal
    Color32::from_rgb(0xB8, 0x73, 0x9A), // mauve
];

pub fn section_color(index: i64) -> Color32 {
    SECTION_COLORS[index.rem_euclid(SECTION_COLORS.len() as i64) as usize]
}

/// Linear blend between two colors (`t` = 0 gives `a`, 1 gives `b`).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

// ---- fonts ------------------------------------------------------------------

pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif".into()))
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 4] = [
        (
            "Inter",
            include_bytes!("../assets/fonts/Inter_400Regular.ttf"),
        ),
        (
            "InterMedium",
            include_bytes!("../assets/fonts/Inter_500Medium.ttf"),
        ),
        (
            "InterSemiBold",
            include_bytes!("../assets/fonts/Inter_600SemiBold.ttf"),
        ),
        (
            "Playfair",
            include_bytes!("../assets/fonts/PlayfairDisplay_600SemiBold.ttf"),
        ),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    // Inter first for normal text; egui's built-in fonts stay as fallbacks for
    // any glyph Inter lacks.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".to_owned());
    for (family, face) in [
        ("medium", "InterMedium"),
        ("semibold", "InterSemiBold"),
        ("serif", "Playfair"),
    ] {
        fonts.families.insert(
            FontFamily::Name(family.into()),
            vec![face.to_owned(), "Inter".to_owned()],
        );
    }
    ctx.set_fonts(fonts);
}

// ---- egui style -------------------------------------------------------------

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(8);
    let hairline = Stroke::new(1.0, p.border);

    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = hairline;
    v.window_corner_radius = CornerRadius::same(16);
    v.menu_corner_radius = CornerRadius::same(12);
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 36,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 120 } else { 40 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 110 } else { 38 }),
    };
    v.faint_bg_color = p.surface_alt;
    v.extreme_bg_color = p.surface_alt;
    v.text_edit_bg_color = Some(p.surface_alt);
    v.code_bg_color = p.surface_alt;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.weak_text_color = Some(p.muted);
    v.striped = false;

    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = hairline;
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = p.surface_alt;
    w.inactive.weak_bg_fill = p.surface_alt;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = p.surface_alt;
    w.hovered.weak_bg_fill = p.surface_alt;
    w.hovered.bg_stroke = Stroke::new(1.0, theme_mix_border(p));
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.accent_soft;
    w.active.weak_bg_fill = p.accent_soft;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.surface_alt;
    w.open.weak_bg_fill = p.surface_alt;
    w.open.bg_stroke = Stroke::new(1.0, p.border);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = radius;
    v
}

fn theme_mix_border(p: &Palette) -> Color32 {
    mix(p.border, p.muted, 0.35)
}

/// Install fonts and both light and dark styles. Call once at startup.
pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_visuals_of(Theme::Light, visuals(&Palette::light()));
    ctx.set_visuals_of(Theme::Dark, visuals(&Palette::dark()));
    ctx.all_styles_mut(|s| {
        use egui::TextStyle::*;
        s.text_styles.insert(Small, body(12.0));
        s.text_styles.insert(Body, body(14.0));
        s.text_styles.insert(Button, medium(14.0));
        s.text_styles.insert(Monospace, FontId::monospace(13.0));
        s.text_styles.insert(Heading, serif(28.0));
        s.spacing.item_spacing = Vec2::new(10.0, 8.0);
        s.spacing.button_padding = Vec2::new(14.0, 7.0);
        s.spacing.interact_size = Vec2::new(36.0, 32.0);
        s.spacing.window_margin = Margin::same(18);
        s.spacing.menu_margin = Margin::same(10);
        s.spacing.indent = 16.0;
    });
}
