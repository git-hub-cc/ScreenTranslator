// 导入Tauri API
const { invoke } = window.__TAURI__.tauri;
const { appWindow } = window.__TAURI__.window;
const { listen } = window.__TAURI__.event; // 明确导入 listen

// --- DOM 元素获取 ---
const shortcutInput = document.getElementById('shortcut-input');
const apiKeyInput = document.getElementById('api-key-input');
const targetLangSelect = document.getElementById('target-lang-select');
const autostartCheckbox = document.getElementById('autostart-checkbox');
const saveBtn = document.getElementById('save-btn');
const statusMessage = document.getElementById('status-message');

// --- 状态变量 ---
let isRecording = false; // 是否正在录制快捷键
let currentSettings = {}; // 当前的应用设置

// --- 函数定义 ---

/**
 * 从后端加载设置并更新UI
 */
async function loadSettings() {
    try {
        const settings = await invoke('get_settings');
        currentSettings = settings;
        console.log("加载到设置:", settings);

        // 更新UI元素的值
        shortcutInput.value = settings.shortcut;
        apiKeyInput.value = settings.api_key;
        targetLangSelect.value = settings.target_lang;
        autostartCheckbox.checked = settings.autostart;

    } catch (error) {
        console.error("加载设置失败:", error);
        showStatusMessage("加载设置失败!", true);
    }
}

/**
 * 保存当前UI上的设置到后端
 */
async function saveSettings() {
    // 从UI元素收集最新的设置值
    const newSettings = {
        shortcut: shortcutInput.value,
        api_key: apiKeyInput.value,
        target_lang: targetLangSelect.value,
        autostart: autostartCheckbox.checked,
    };

    try {
        // 调用后端指令保存设置
        await invoke('set_settings', { settings: newSettings });
        showStatusMessage("设置已保存!", false);
        currentSettings = newSettings; // 更新本地缓存的设置
    } catch (error) {
        console.error("保存设置失败:", error);
        showStatusMessage("保存设置失败!", true);
    }
}

/**
 * 在界面上显示状态消息
 * @param {string} msg - 要显示的消息
 * @param {boolean} isError - 是否是错误消息
 */
function showStatusMessage(msg, isError = false) {
    statusMessage.textContent = msg;
    statusMessage.style.color = isError ? 'var(--error-color)' : 'var(--accent-color)';
    // 3秒后自动清除消息
    setTimeout(() => {
        statusMessage.textContent = '';
    }, 3000);
}


/**
 * 格式化并显示快捷键
 * @param {KeyboardEvent} e - 键盘事件对象
 * @returns {string} 格式化后的快捷键字符串，如 "Alt+Q"
 */
function formatShortcut(e) {
    const parts = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Super'); // Meta键在Windows上是Win键，在macOS上是Cmd键

    const key = e.key.toLowerCase();
    if (!['control', 'alt', 'shift', 'meta'].includes(key)) {
        parts.push(e.code.replace('Key', '').replace('Digit', ''));
    }

    return parts.join('+');
}


// --- 事件监听 ---

// 保存按钮点击事件
saveBtn.addEventListener('click', saveSettings);

// --- 快捷键录制逻辑 ---
shortcutInput.addEventListener('focus', () => {
    isRecording = true;
    shortcutInput.value = '请按下快捷键...';
});

shortcutInput.addEventListener('blur', () => {
    isRecording = false;
    if (shortcutInput.value === '请按下快捷键...') {
        shortcutInput.value = currentSettings.shortcut || '';
    }
});

shortcutInput.addEventListener('keydown', (e) => {
    if (isRecording) {
        e.preventDefault();
        const formatted = formatShortcut(e);
        if (formatted.includes('+') && formatted.split('+').length > 1) {
            shortcutInput.value = formatted;
            shortcutInput.blur();
        }
    }
});

// --- 核心修正：使用事件监听代替 DOMContentLoaded ---
// 等待后端通过事件通知我们它已经准备就绪，然后再去加载设置。
// 这样可以 100% 保证在调用 `invoke('get_settings')` 时，后端的 `AppState` 已经被 `manage()`。
listen('backend-ready', () => {
    console.log("接收到 'backend-ready' 事件，开始加载设置...");
    loadSettings();
});