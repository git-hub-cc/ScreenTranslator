// 导入Tauri API
const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;

// --- DOM 元素获取 ---
const shortcutInput = document.getElementById('shortcut-input');
const viewShortcutInput = document.getElementById('view-shortcut-input');
const targetLangSelect = document.getElementById('target-lang-select');
const targetLangContainer = document.getElementById('target-lang-container');
const lineBreakCheckbox = document.getElementById('line-break-checkbox');
const ocrSettingsBlock = document.getElementById('ocr-settings-block');

// 获取所有的单选按钮
const radioInputs = document.getElementsByName('primary-action');

// --- 状态与默认值 ---
shortcutInput.value = 'F1';
viewShortcutInput.value = 'F3';
let isRecording = { main: false, view: false };
let currentSettings = {};

// --- 函数定义 ---

/**
 * 根据选择的“首要动作”动态更新UI显示状态
 * (渐进式显示设置项)
 */
function updateUIBasedOnAction(actionValue) {
    console.log("切换首要动作:", actionValue);

    // 1. 控制“识别与翻译设置”区块的显示
    // 只有在选择 "ocr" 或 "ocr_translate" 时才显示此区块
    if (actionValue === 'ocr' || actionValue === 'ocr_translate') {
        ocrSettingsBlock.classList.remove('hidden');
    } else {
        ocrSettingsBlock.classList.add('hidden');
    }

    // 2. 控制“目标语言”选项的显示
    // 只有在选择 "ocr_translate" 时，才需要选择目标语言
    if (actionValue === 'ocr_translate') {
        targetLangContainer.classList.remove('hidden');
    } else {
        targetLangContainer.classList.add('hidden');
    }
}

/**
 * 从后端加载设置并更新UI
 */
async function loadSettings() {
    try {
        const settings = await invoke('get_settings');
        currentSettings = settings;
        console.log("加载到设置:", settings);

        // 更新快捷键输入框
        shortcutInput.value = settings.shortcut;
        viewShortcutInput.value = settings.view_image_shortcut;

        // 更新下拉菜单和复选框
        targetLangSelect.value = settings.target_lang;
        lineBreakCheckbox.checked = settings.preserve_line_breaks;

        // 更新单选按钮组选中状态
        for (const radio of radioInputs) {
            if (radio.value === settings.primary_action) {
                radio.checked = true;
                // 初始化时触发一次UI更新
                updateUIBasedOnAction(radio.value);
                break;
            }
        }

    } catch (error) {
        console.error("加载设置失败:", error);
    }
}

/**
 * 保存当前UI上的设置到后端
 * (此函数现在直接由控件事件触发)
 */
async function saveSettings() {
    // 验证快捷键
    const shortcutValue = shortcutInput.value.trim();
    if (!shortcutValue) {
        console.warn("截图快捷键不能为空！恢复旧值。");
        shortcutInput.value = currentSettings.shortcut || 'F1';
        return;
    }
    const viewShortcutValue = viewShortcutInput.value.trim();
    if (!viewShortcutValue) {
        console.warn("查看截图快捷键不能为空！恢复旧值。");
        viewShortcutInput.value = currentSettings.view_image_shortcut || 'F3';
        return;
    }

    // 获取当前选中的首要动作
    let selectedAction = 'ocr'; // 默认安全值
    for (const radio of radioInputs) {
        if (radio.checked) {
            selectedAction = radio.value;
            break;
        }
    }

    // 构造设置对象
    const newSettings = {
        shortcut: shortcutValue,
        view_image_shortcut: viewShortcutValue,
        target_lang: targetLangSelect.value,
        // autostart 已移除
        preserve_line_breaks: lineBreakCheckbox.checked,
        // 首要动作字段
        primary_action: selectedAction,
        // 旧字段保留默认值
        enable_ocr: false,
        enable_translation: false
    };

    try {
        await invoke('set_settings', { settings: newSettings });
        console.log("设置已保存:", newSettings);
        currentSettings = newSettings;
    } catch (error) {
        console.error("保存设置失败:", error);
        // 可选：在这里弹出系统通知告知用户保存失败
    }
}

/**
 * 格式化并显示快捷键
 */
function formatShortcut(e) {
    const parts = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Super');

    const key = e.key.toLowerCase();
    if (!['control', 'alt', 'shift', 'meta'].includes(key)) {
        parts.push(e.code.replace('Key', '').replace('Digit', ''));
    }

    return parts.join('+');
}


// --- 事件监听 (即时保存逻辑) ---

// 1. 监听单选按钮的变化，实时更新UI并保存
for (const radio of radioInputs) {
    radio.addEventListener('change', (e) => {
        if (e.target.checked) {
            updateUIBasedOnAction(e.target.value);
            saveSettings(); // 触发保存
        }
    });
}

// 2. 监听复选框和下拉菜单的变化，触发保存
targetLangSelect.addEventListener('change', saveSettings);
lineBreakCheckbox.addEventListener('change', saveSettings);

// 3. 快捷键录制逻辑
// (只有在输入框失去焦点 'blur' 时才触发保存，避免输入过程中频繁保存)

shortcutInput.addEventListener('focus', () => {
    isRecording.main = true;
    shortcutInput.value = '请按下快捷键...';
});
shortcutInput.addEventListener('blur', () => {
    isRecording.main = false;
    if (shortcutInput.value === '请按下快捷键...') {
        shortcutInput.value = currentSettings.shortcut || 'F1';
    }
    saveSettings(); // 触发保存
});
shortcutInput.addEventListener('keydown', (e) => {
    if (isRecording.main) {
        e.preventDefault();
        const formatted = formatShortcut(e);
        if (formatted && (formatted.includes('+') || formatted.startsWith('F'))) {
            shortcutInput.value = formatted;
            shortcutInput.blur();
        }
    }
});

viewShortcutInput.addEventListener('focus', () => {
    isRecording.view = true;
    viewShortcutInput.value = '请按下快捷键...';
});
viewShortcutInput.addEventListener('blur', () => {
    isRecording.view = false;
    if (viewShortcutInput.value === '请按下快捷键...') {
        viewShortcutInput.value = currentSettings.view_image_shortcut || 'F3';
    }
    saveSettings(); // 触发保存
});
viewShortcutInput.addEventListener('keydown', (e) => {
    if (isRecording.view) {
        e.preventDefault();
        const formatted = formatShortcut(e);
        if (formatted && (formatted.includes('+') || formatted.startsWith('F'))) {
            viewShortcutInput.value = formatted;
            viewShortcutInput.blur();
        }
    }
});

// --- 初始化 ---
listen('backend-ready', () => {
    console.log("接收到 'backend-ready' 事件，开始加载设置...");
    loadSettings();
});