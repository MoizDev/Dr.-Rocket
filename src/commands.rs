//! Every user-invocable command, with its label, icon and default shortcut. The command
//! palette, menus and keyboard handling all read from this table.

use crate::ui::ic;
use egui::{Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    // File
    NewFile,
    OpenFile,
    OpenFolder,
    Save,
    SaveAs,
    SaveAll,
    CloseTab,
    ReopenClosedTab,
    CloseFolder,
    RevealInFinder,
    // Navigation
    Palette,
    QuickOpen,
    GoToLine,
    GoToSymbol,
    NextTab,
    PrevTab,
    NextProblem,
    // View
    ToggleLeftPanel,
    ToggleRightPanel,
    ToggleBottomPanel,
    ShowFiles,
    ShowOutline,
    ShowSearch,
    FocusEditor,
    FocusInteractions,
    NewTerminal,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    SelectTheme,
    ToggleRainbowBrackets,
    ToggleBindingArrows,
    MoveInteractions,
    // Racket
    Run,
    Break,
    Kill,
    Stepper,
    Visualize,
    Debug,
    CheckSyntax,
    MacroStepper,
    Profile,
    RunWithCoverage,
    ChooseLanguage,
    SearchDocs,
    ClearInteractions,
    CreateExecutable,
    InstallPackage,
    UpdatePackage,
    RemovePackage,
    ShowPackages,
    Revert,
    SaveInteractions,
    Print,
    ModuleBrowser,
    RunTests,
    InsertLambda,
    InsertImage,
    ReindentAll,
    // App
    Settings,
    KeyboardShortcuts,
    About,
}

pub struct CommandInfo {
    pub cmd: Command,
    pub label: &'static str,
    pub group: &'static str,
    pub icon: &'static str,
    pub shortcut: Option<KeyboardShortcut>,
}

const CMD: Modifiers = Modifiers::COMMAND;
const fn cmd_shift() -> Modifiers {
    Modifiers { alt: false, ctrl: false, shift: true, mac_cmd: false, command: true }
}
const fn cmd_alt() -> Modifiers {
    Modifiers { alt: true, ctrl: false, shift: false, mac_cmd: false, command: true }
}
const fn ctrl() -> Modifiers {
    Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: false }
}
const fn sc(m: Modifiers, k: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(m, k))
}

pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo { cmd: Command::Run, label: "Run", group: "Racket", icon: ic::PLAY_ARROW, shortcut: sc(CMD, Key::R) },
    CommandInfo { cmd: Command::Break, label: "Break", group: "Racket", icon: ic::PAUSE, shortcut: sc(CMD, Key::Period) },
    CommandInfo { cmd: Command::Kill, label: "Kill Program", group: "Racket", icon: ic::STOP, shortcut: sc(CMD, Key::K) },
    CommandInfo { cmd: Command::Stepper, label: "Step Through Program (Stepper)", group: "Racket", icon: ic::STEP, shortcut: sc(cmd_shift(), Key::E) },
    CommandInfo { cmd: Command::Visualize, label: "Visualize Program", group: "Racket", icon: ic::ACCOUNT_TREE, shortcut: sc(cmd_shift(), Key::V) },
    CommandInfo { cmd: Command::Debug, label: "Debug", group: "Racket", icon: ic::BUG_REPORT, shortcut: sc(cmd_shift(), Key::D) },
    CommandInfo { cmd: Command::CheckSyntax, label: "Check Syntax", group: "Racket", icon: ic::CHECK_CIRCLE, shortcut: sc(cmd_shift(), Key::C) },
    CommandInfo { cmd: Command::MacroStepper, label: "Macro Stepper", group: "Racket", icon: ic::AUTO_FIX_HIGH, shortcut: sc(cmd_shift(), Key::M) },
    CommandInfo { cmd: Command::Profile, label: "Run with Profiling", group: "Racket", icon: ic::SPEED, shortcut: None },
    CommandInfo { cmd: Command::RunWithCoverage, label: "Run with Test Coverage", group: "Racket", icon: ic::RULE, shortcut: None },
    CommandInfo { cmd: Command::RunTests, label: "Run Tests", group: "Racket", icon: ic::SCIENCE, shortcut: sc(cmd_alt(), Key::T) },
    CommandInfo { cmd: Command::ChooseLanguage, label: "Choose Language…", group: "Racket", icon: ic::SCHOOL, shortcut: sc(CMD, Key::L) },
    CommandInfo { cmd: Command::SearchDocs, label: "Search Racket Documentation…", group: "Racket", icon: ic::MENU_BOOK, shortcut: sc(cmd_shift(), Key::H) },
    CommandInfo { cmd: Command::ClearInteractions, label: "Clear Interactions", group: "Racket", icon: ic::CLEANING_SERVICES, shortcut: None },
    CommandInfo { cmd: Command::InsertLambda, label: "Insert λ", group: "Racket", icon: ic::FUNCTION, shortcut: None },
    CommandInfo { cmd: Command::InsertImage, label: "Insert Image…", group: "Racket", icon: ic::FILE_OPEN, shortcut: None },
    CommandInfo { cmd: Command::ReindentAll, label: "Reindent All", group: "Racket", icon: ic::FORMAT_ALIGN_LEFT, shortcut: None },
    CommandInfo { cmd: Command::ModuleBrowser, label: "Module Browser", group: "Racket", icon: ic::ACCOUNT_TREE, shortcut: None },
    CommandInfo { cmd: Command::CreateExecutable, label: "Create Executable…", group: "Racket", icon: ic::ROCKET_LAUNCH, shortcut: None },
    CommandInfo { cmd: Command::InstallPackage, label: "Install Package…", group: "Racket", icon: ic::PACKAGE_2, shortcut: None },
    CommandInfo { cmd: Command::UpdatePackage, label: "Update Package…", group: "Racket", icon: ic::PACKAGE_2, shortcut: None },
    CommandInfo { cmd: Command::RemovePackage, label: "Remove Package…", group: "Racket", icon: ic::PACKAGE_2, shortcut: None },
    CommandInfo { cmd: Command::ShowPackages, label: "Show Installed Packages", group: "Racket", icon: ic::PACKAGE_2, shortcut: None },
    CommandInfo { cmd: Command::Revert, label: "Revert to Saved", group: "File", icon: ic::HISTORY, shortcut: None },
    CommandInfo { cmd: Command::SaveInteractions, label: "Save Interactions As…", group: "File", icon: ic::SAVE, shortcut: None },
    CommandInfo { cmd: Command::Print, label: "Print…", group: "File", icon: ic::DESCRIPTION, shortcut: None },
    CommandInfo { cmd: Command::NewFile, label: "New File", group: "File", icon: ic::NOTE_ADD, shortcut: sc(CMD, Key::N) },
    CommandInfo { cmd: Command::OpenFile, label: "Open File…", group: "File", icon: ic::FILE_OPEN, shortcut: sc(CMD, Key::O) },
    CommandInfo { cmd: Command::OpenFolder, label: "Open Folder…", group: "File", icon: ic::FOLDER_OPEN, shortcut: sc(cmd_shift(), Key::O) },
    CommandInfo { cmd: Command::Save, label: "Save", group: "File", icon: ic::SAVE, shortcut: sc(CMD, Key::S) },
    CommandInfo { cmd: Command::SaveAs, label: "Save As…", group: "File", icon: ic::SAVE, shortcut: sc(cmd_shift(), Key::S) },
    CommandInfo { cmd: Command::SaveAll, label: "Save All", group: "File", icon: ic::SAVE, shortcut: sc(cmd_alt(), Key::S) },
    CommandInfo { cmd: Command::CloseTab, label: "Close Tab", group: "File", icon: ic::CLOSE, shortcut: sc(CMD, Key::W) },
    CommandInfo { cmd: Command::ReopenClosedTab, label: "Reopen Closed Tab", group: "File", icon: ic::HISTORY, shortcut: sc(cmd_shift(), Key::T) },
    CommandInfo { cmd: Command::CloseFolder, label: "Close Folder", group: "File", icon: ic::FOLDER, shortcut: None },
    CommandInfo { cmd: Command::RevealInFinder, label: "Reveal in Finder", group: "File", icon: ic::OPEN_IN_NEW, shortcut: None },
    CommandInfo { cmd: Command::Palette, label: "Command Palette", group: "Go", icon: ic::KEYBOARD_COMMAND_KEY, shortcut: sc(cmd_shift(), Key::P) },
    CommandInfo { cmd: Command::QuickOpen, label: "Go to File…", group: "Go", icon: ic::SEARCH, shortcut: sc(CMD, Key::P) },
    CommandInfo { cmd: Command::GoToLine, label: "Go to Line…", group: "Go", icon: ic::FORMAT_LIST_NUMBERED, shortcut: sc(ctrl(), Key::G) },
    CommandInfo { cmd: Command::GoToSymbol, label: "Go to Definition in File…", group: "Go", icon: ic::FUNCTION, shortcut: sc(cmd_shift(), Key::Period) },
    CommandInfo { cmd: Command::NextTab, label: "Next Tab", group: "Go", icon: ic::CHEVRON_RIGHT, shortcut: sc(cmd_alt(), Key::ArrowRight) },
    CommandInfo { cmd: Command::PrevTab, label: "Previous Tab", group: "Go", icon: ic::CHEVRON_LEFT, shortcut: sc(cmd_alt(), Key::ArrowLeft) },
    CommandInfo { cmd: Command::NextProblem, label: "Next Problem", group: "Go", icon: ic::ERROR, shortcut: sc(Modifiers::NONE, Key::F8) },
    CommandInfo { cmd: Command::ToggleLeftPanel, label: "Toggle Left Panel", group: "View", icon: ic::LEFT_PANEL_OPEN, shortcut: sc(CMD, Key::B) },
    CommandInfo { cmd: Command::ToggleRightPanel, label: "Toggle Interactions Panel", group: "View", icon: ic::RIGHT_PANEL_OPEN, shortcut: sc(CMD, Key::E) },
    CommandInfo { cmd: Command::ToggleBottomPanel, label: "Toggle Bottom Panel", group: "View", icon: ic::BOTTOM_PANEL_OPEN, shortcut: sc(CMD, Key::J) },
    CommandInfo { cmd: Command::ShowFiles, label: "Show Files", group: "View", icon: ic::FOLDER, shortcut: None },
    CommandInfo { cmd: Command::ShowOutline, label: "Show Outline", group: "View", icon: ic::TOC, shortcut: None },
    CommandInfo { cmd: Command::ShowSearch, label: "Search in Project", group: "View", icon: ic::MANAGE_SEARCH, shortcut: sc(cmd_shift(), Key::F) },
    CommandInfo { cmd: Command::FocusEditor, label: "Focus Definitions", group: "View", icon: ic::CODE, shortcut: sc(CMD, Key::Num1) },
    CommandInfo { cmd: Command::FocusInteractions, label: "Focus Interactions", group: "View", icon: ic::TERMINAL, shortcut: sc(CMD, Key::Num2) },
    CommandInfo { cmd: Command::NewTerminal, label: "New Terminal", group: "View", icon: ic::TERMINAL, shortcut: sc(ctrl(), Key::Backtick) },
    CommandInfo { cmd: Command::MoveInteractions, label: "Move Interactions Below/Beside Definitions", group: "View", icon: ic::SWAP_HORIZ, shortcut: None },
    CommandInfo { cmd: Command::ZoomIn, label: "Zoom In", group: "View", icon: ic::ZOOM_IN, shortcut: sc(CMD, Key::Equals) },
    CommandInfo { cmd: Command::ZoomOut, label: "Zoom Out", group: "View", icon: ic::ZOOM_OUT, shortcut: sc(CMD, Key::Minus) },
    CommandInfo { cmd: Command::ZoomReset, label: "Reset Zoom", group: "View", icon: ic::FORMAT_SIZE, shortcut: sc(CMD, Key::Num0) },
    CommandInfo { cmd: Command::SelectTheme, label: "Select Theme…", group: "View", icon: ic::PALETTE, shortcut: sc(cmd_shift(), Key::K) },
    CommandInfo { cmd: Command::ToggleRainbowBrackets, label: "Toggle Rainbow Brackets", group: "View", icon: ic::DATA_ARRAY, shortcut: None },
    CommandInfo { cmd: Command::ToggleBindingArrows, label: "Toggle Binding Arrows", group: "View", icon: ic::EAST, shortcut: None },
    CommandInfo { cmd: Command::Settings, label: "Settings", group: "App", icon: ic::SETTINGS, shortcut: sc(CMD, Key::Comma) },
    CommandInfo { cmd: Command::KeyboardShortcuts, label: "Keyboard Shortcuts", group: "App", icon: ic::KEYBOARD, shortcut: None },
    CommandInfo { cmd: Command::About, label: "About Dr. Rocket", group: "App", icon: ic::INFO, shortcut: None },
];

/// The native menu bar: commands grouped into menus (`None` = separator).
pub fn menu_specs() -> Vec<crate::platform::MenuSpec> {
    use crate::platform::{MenuAction, MenuEntry, MenuSpec};
    let entry = |cmd: Command| {
        let i = COMMANDS.iter().position(|c| c.cmd == cmd).expect("command in table");
        Some(MenuEntry { label: COMMANDS[i].label.to_string(), action: MenuAction::Command(i), shortcut: COMMANDS[i].shortcut })
    };
    let about = entry(Command::About).map(|mut e| {
        e.label = format!("About {}", crate::ui::brand::APP_NAME);
        e
    });
    let settings = entry(Command::Settings).map(|mut e| {
        e.label = "Settings…".into();
        e
    });
    vec![
        MenuSpec { title: "App", entries: vec![about, None, settings, entry(Command::SelectTheme), entry(Command::KeyboardShortcuts)] },
        MenuSpec {
            title: "File",
            entries: vec![
                entry(Command::NewFile),
                entry(Command::OpenFile),
                entry(Command::OpenFolder),
                None,
                entry(Command::Save),
                entry(Command::SaveAs),
                entry(Command::SaveAll),
                entry(Command::Revert),
                None,
                entry(Command::SaveInteractions),
                entry(Command::Print),
                None,
                entry(Command::CloseTab),
                entry(Command::ReopenClosedTab),
                entry(Command::CloseFolder),
                None,
                entry(Command::RevealInFinder),
            ],
        },
        MenuSpec {
            title: "View",
            entries: vec![
                entry(Command::Palette),
                None,
                entry(Command::ToggleLeftPanel),
                entry(Command::ToggleRightPanel),
                entry(Command::ToggleBottomPanel),
                entry(Command::MoveInteractions),
                None,
                entry(Command::ShowFiles),
                entry(Command::ShowOutline),
                entry(Command::ShowSearch),
                entry(Command::NewTerminal),
                None,
                entry(Command::ZoomIn),
                entry(Command::ZoomOut),
                entry(Command::ZoomReset),
                None,
                entry(Command::ToggleRainbowBrackets),
                entry(Command::ToggleBindingArrows),
            ],
        },
        MenuSpec {
            title: "Racket",
            entries: vec![
                entry(Command::Run),
                entry(Command::Break),
                entry(Command::Kill),
                None,
                entry(Command::Stepper),
                entry(Command::Visualize),
                entry(Command::Debug),
                entry(Command::CheckSyntax),
                entry(Command::MacroStepper),
                entry(Command::RunTests),
                entry(Command::RunWithCoverage),
                entry(Command::Profile),
                entry(Command::ModuleBrowser),
                None,
                entry(Command::ChooseLanguage),
                entry(Command::ClearInteractions),
                entry(Command::ReindentAll),
                entry(Command::InsertLambda),
                entry(Command::InsertImage),
                None,
                entry(Command::CreateExecutable),
                entry(Command::InstallPackage),
                entry(Command::UpdatePackage),
                entry(Command::RemovePackage),
                entry(Command::ShowPackages),
            ],
        },
        MenuSpec {
            title: "Go",
            entries: vec![
                entry(Command::QuickOpen),
                entry(Command::GoToLine),
                entry(Command::GoToSymbol),
                None,
                entry(Command::NextTab),
                entry(Command::PrevTab),
                entry(Command::NextProblem),
                None,
                entry(Command::FocusEditor),
                entry(Command::FocusInteractions),
            ],
        },
        MenuSpec { title: "Window", entries: vec![] },
        MenuSpec { title: "Help", entries: vec![entry(Command::SearchDocs), entry(Command::KeyboardShortcuts)] },
    ]
}

pub fn info(cmd: Command) -> &'static CommandInfo {
    COMMANDS.iter().find(|c| c.cmd == cmd).expect("every command has info")
}

/// Human-readable shortcut, macOS style (⌘⇧R).
pub fn shortcut_text(s: &KeyboardShortcut) -> String {
    let mut out = String::new();
    let m = s.modifiers;
    if m.ctrl {
        out.push('⌃');
    }
    if m.alt {
        out.push('⌥');
    }
    if m.shift {
        out.push('⇧');
    }
    if m.command || m.mac_cmd {
        out.push('⌘');
    }
    let k = match s.logical_key {
        Key::ArrowLeft => "←".to_string(),
        Key::ArrowRight => "→".to_string(),
        Key::ArrowUp => "↑".to_string(),
        Key::ArrowDown => "↓".to_string(),
        Key::Backtick => "`".to_string(),
        Key::Period => ".".to_string(),
        Key::Comma => ",".to_string(),
        Key::Equals => "=".to_string(),
        Key::Minus => "-".to_string(),
        other => other.name().to_string(),
    };
    out.push_str(&k);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn shortcuts_are_unique() {
        let mut seen = HashSet::new();
        for c in COMMANDS {
            if let Some(s) = c.shortcut {
                assert!(seen.insert((s.modifiers.command, s.modifiers.shift, s.modifiers.alt, s.modifiers.ctrl, s.logical_key)), "duplicate shortcut for {}", c.label);
            }
        }
    }
}
