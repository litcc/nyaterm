//! Shared GPUI theme tokens and reusable presentation widgets for NyaTerm.

mod app_menu_bar;
mod button;
mod dialog;
mod input;
mod input_focus;
mod menu;
mod number_input;
mod popover;
mod root;
mod selectable_text;
mod selection;
mod settings;
mod sizing;
mod tabs;
mod theme;
mod theme_bridge;
mod tooltip;
mod widgets;

pub use app_menu_bar::{NyaAppMenu, NyaAppMenuBar};
pub use button::{NyaButton, NyaButtonVariant, NyaIconButton};
pub use dialog::{NyaConfirmDialog, NyaDialog, NyaDialogFooter, NyaDialogWindowExt};
pub use gpui_component::input::{
    Copy as NyaCopy, Cut as NyaCut, Paste as NyaPaste, Redo as NyaRedo, SelectAll as NyaSelectAll,
    Undo as NyaUndo,
};
pub use gpui_component::scroll::ScrollableElement as NyaScrollable;
pub use gpui_component::scroll::ScrollbarAxis as NyaScrollbarAxis;
pub use input::{
    NyaInput, NyaInputEvent, NyaInputShell, NyaInputState, NyaSearchInput, NyaTextArea,
};
pub use menu::{NyaContextMenu, NyaDropdownMenu, NyaMenuAnchor, NyaMenuItem};
pub use number_input::{
    NyaNumberInput, NyaNumberInputEvent, NyaNumberInputOptions, NyaNumberInputState, NyaNumberStep,
};
pub use popover::NyaPopover;
pub use root::{NyaRoot, NyaWindowHandle, nya_root};
pub use selectable_text::NyaSelectableText;
pub use selection::{
    NyaCheckbox, NyaRadioGroup, NyaSelect, NyaSelectEvent, NyaSelectOption, NyaSelectState,
    NyaSwitch,
};
pub use settings::{NyaSettingsLayout, NyaSettingsNavGroup, NyaSettingsNavItem};
pub use sizing::NYA_FORM_CONTROL_HEIGHT_PX;
pub use tabs::{NyaTabItem, NyaTabs, NyaTabsVariant};
pub use theme::{APPEARANCE_THEME_IDS, ThemePalette, appearance_theme_label, theme_palette};
pub use theme_bridge::apply_component_theme;
pub use tooltip::NyaTooltip;
pub use widgets::{
    NyaHorizontalScrollbar, NyaScrollArea, NyaUniformListScrollbar, capability_line, empty_panel,
    mode_button, section_header, session_info_row, small_button, status_pill, svg_icon_button,
};
