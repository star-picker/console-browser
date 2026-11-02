//! 动作：键位翻译成动作，App 再执行动作。键位与业务互不认识，方便测。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // 输入
    FocusOmnibox,
    FocusFind,
    FocusCommand,
    Submit,
    CancelInput,
    ClearInput,
    EditBackspace,
    EditWordBackspace,
    EditLeft,
    EditRight,
    EditHome,
    EditEnd,
    HistoryPrev,
    HistoryNext,

    // 导航
    Open(String),
    Back,
    Forward,
    Reload,
    Home,

    // 页面
    ScrollDown(i32),
    ScrollUp(i32),
    PageDown,
    PageTop,
    PageBottom,
    FocusNextLink,
    FocusPrevLink,
    OpenFocusedLink { new_tab: bool },
    ToggleRawView,
    ToggleMarkdownView,

    // 查找
    FindNext,
    FindPrev,

    // 标签
    TabNew,
    TabClose,
    TabNext,
    TabPrev,
    TabSelect(usize),

    // 浮层
    ToggleHelp,
    ToggleHistory,
    ToggleLinks,
    TogglePalette,
    ToggleAi,
    ToggleTrace,
    OverlayUp,
    OverlayDown,
    OverlaySelect,
    OverlayFilter,
    CloseOverlay,

    // 工具
    OpenHumanAssist,
    PasteCookies,
    ExportMarkdown,
    ExportHtml,
    CopyUrl,
    SwitchLang,
    Quit,
    ForceQuit,

    // 鼠标（由视图层翻译）
    MouseClick(u16, u16),
    MouseScroll(bool),
}
