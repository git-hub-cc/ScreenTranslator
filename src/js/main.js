// --- 文件: src/js/main.js ---

// 导入 Tauri 核心 API，用于与 Rust 后端进行交互
const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;
const { message, confirm } = window.__TAURI__.dialog;

// --- DOM 元素获取 ---
// 将页面上所有需要操作的 HTML 元素预先获取并存入变量，方便后续使用

// 常规设置元素
const shortcutInput = document.getElementById('shortcut-input');
const viewShortcutInput = document.getElementById('view-shortcut-input');
const targetLangSelect = document.getElementById('target-lang-select');
const targetLangContainer = document.getElementById('target-lang-container');
const lineBreakCheckbox = document.getElementById('line-break-checkbox');
const ocrSettingsBlock = document.getElementById('ocr-settings-block');
const radioInputs = document.getElementsByName('primary-action');

// OCR 引擎管理相关元素
const ocrEngineStatusBadge = document.getElementById('ocr-engine-status');
const downloadOcrBtn = document.getElementById('download-ocr-btn');
const ocrProgressContainer = document.getElementById('ocr-progress-container');
const ocrProgressBar = document.getElementById('ocr-download-progress');
const ocrProgressLabel = document.getElementById('ocr-progress-label');

// 翻译引擎管理相关元素
const engineStatusBadge = document.getElementById('engine-status');
const downloadBtn = document.getElementById('download-btn');
const progressContainer = document.getElementById('progress-container');
const progressBar = document.getElementById('download-progress');
const progressLabel = document.getElementById('progress-label');

// --- 全局状态与默认值 ---
// 用于管理前端 UI 状态和缓存数据

// 快捷键录制状态
let isRecording = { main: false, view: false };
// 当前从后端加载的设置，用于比对和恢复
let currentSettings = {};
// 引擎安装状态标志
let isOcrInstalled = false;
let isTranslatorInstalled = false;
// 引擎下载状态标志，防止重复点击
let isOcrDownloading = false;
let isTranslatorDownloading = false;

// --- 函数定义 ---

/**
 * 检查本地 OCR 引擎的安装状态。
 * @async
 */
async function checkOcrStatus() {
    console.log("[前端] 发起 OCR 状态检查...");
    try {
        // 调用 Rust 后端的 `check_ocr_status` 命令
        isOcrInstalled = await invoke('check_ocr_status');
        console.log("[前端] 收到 OCR 状态: isOcrInstalled =", isOcrInstalled);
        // 根据检查结果更新 UI 显示
        updateOcrUI();
    } catch (e) {
        console.error("[前端] 检查OCR引擎状态失败:", e);
        ocrEngineStatusBadge.textContent = "检查失败";
        ocrEngineStatusBadge.className = "status-badge missing";
    }
}

/**
 * 根据 OCR 引擎的安装状态，更新相关的 UI 元素（状态徽章、按钮文本）。
 */
function updateOcrUI() {
    if (isOcrInstalled) {
        ocrEngineStatusBadge.textContent = "已安装";
        ocrEngineStatusBadge.className = "status-badge installed";
        downloadOcrBtn.textContent = "重新下载 / 更新";
    } else {
        ocrEngineStatusBadge.textContent = "未安装";
        ocrEngineStatusBadge.className = "status-badge missing";
        downloadOcrBtn.textContent = "立即下载安装";
    }
}

/**
 * 检查本地翻译引擎的安装状态。
 * @async
 */
async function checkTranslatorStatus() {
    try {
        isTranslatorInstalled = await invoke('check_translator_status');
        updateTranslatorUI();
    } catch (e) {
        console.error("检查翻译引擎状态失败:", e);
        engineStatusBadge.textContent = "检查失败";
        engineStatusBadge.className = "status-badge missing";
    }
}

/**
 * 根据翻译引擎的安装状态，更新相关的 UI 元素。
 */
function updateTranslatorUI() {
    if (isTranslatorInstalled) {
        engineStatusBadge.textContent = "已安装";
        engineStatusBadge.className = "status-badge installed";
        downloadBtn.textContent = "重新下载 / 更新";
    } else {
        engineStatusBadge.textContent = "未安装";
        engineStatusBadge.className = "status-badge missing";
        downloadBtn.textContent = "立即下载安装";
    }
}


/**
 * 根据用户选择的“首要动作”，动态显示或隐藏相关的设置项。
 * @param {string} actionValue - 用户选择的动作值 (e.g., 'ocr', 'ocr_translate')。
 */
function updateUIBasedOnAction(actionValue) {
    const requiresOcr = ['ocr', 'ocr_translate', 'preview'].includes(actionValue);
    const requiresTranslation = actionValue === 'ocr_translate';

    // 1. 如果动作需要 OCR，则显示“识别与翻译设置”区块
    ocrSettingsBlock.classList.toggle('hidden', !requiresOcr);

    // 2. 如果动作需要翻译，则显示“目标语言”下拉框
    targetLangContainer.classList.toggle('hidden', !requiresTranslation);

    // 3. 智能提示：如果用户选择了需要引擎的功能但未安装，则高亮下载按钮
    if (requiresOcr && !isOcrInstalled) {
        downloadOcrBtn.style.boxShadow = "0 0 8px #ff3b30";
        setTimeout(() => downloadOcrBtn.style.boxShadow = "", 1500);
    }
    if (requiresTranslation && !isTranslatorInstalled) {
        downloadBtn.style.boxShadow = "0 0 8px #ff3b30";
        setTimeout(() => downloadBtn.style.boxShadow = "", 1500);
    }
}

/**
 * 从后端加载应用设置，并更新整个 UI 界面以反映这些设置。
 * @async
 */
async function loadSettings() {
    try {
        const settings = await invoke('get_settings');
        currentSettings = settings; // 缓存设置

        // 更新各个表单控件的值
        shortcutInput.value = settings.shortcut;
        viewShortcutInput.value = settings.view_image_shortcut;
        targetLangSelect.value = settings.target_lang;
        lineBreakCheckbox.checked = settings.preserve_line_breaks;

        // 根据加载的 'primary_action' 设置单选框的选中状态
        for (const radio of radioInputs) {
            if (radio.value === settings.primary_action) {
                radio.checked = true;
                // 触发一次 UI 更新，以确保依赖于此选项的其它设置项正确显示/隐藏
                updateUIBasedOnAction(radio.value);
                break;
            }
        }
    } catch (error) {
        console.error("加载设置失败:", error);
        // 此处可以添加用户提示，例如弹窗告知加载失败
    }
}

/**
 * 收集当前 UI 上的所有设置，并将其保存到后端。
 * @async
 */
async function saveSettings() {
    // 简单校验快捷键输入框，防止为空
    const shortcutValue = shortcutInput.value.trim();
    if (!shortcutValue) {
        shortcutInput.value = currentSettings.shortcut || 'F1'; // 恢复为上次的值
        return;
    }
    const viewShortcutValue = viewShortcutInput.value.trim();
    if (!viewShortcutValue) {
        viewShortcutInput.value = currentSettings.view_image_shortcut || 'F3'; // 恢复
        return;
    }

    // 获取当前选中的“首要动作”
    let selectedAction = 'ocr';
    for (const radio of radioInputs) {
        if (radio.checked) {
            selectedAction = radio.value;
            break;
        }
    }

    // 构造新的设置对象
    const newSettings = {
        shortcut: shortcutValue,
        view_image_shortcut: viewShortcutValue,
        target_lang: targetLangSelect.value,
        preserve_line_breaks: lineBreakCheckbox.checked,
        primary_action: selectedAction,
    };

    try {
        // 调用后端 `set_settings` 命令
        await invoke('set_settings', { settings: newSettings });
        currentSettings = newSettings; // 更新本地缓存
    } catch (error) {
        console.error("保存设置失败:", error);
    }
}

/**
 * 格式化键盘事件，生成可读的快捷键字符串 (e.g., "Ctrl+Alt+A")。
 * @param {KeyboardEvent} e - 键盘事件对象。
 * @returns {string} 格式化后的快捷键字符串。
 */
function formatShortcut(e) {
    const parts = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Super'); // 'Super' 对应 Windows 键或 Command 键

    const key = e.key.toLowerCase();
    // 避免重复添加修饰键
    if (!['control', 'alt', 'shift', 'meta'].includes(key)) {
        // 使用 e.code 以获得更准确的按键表示，如 "KeyA" -> "A"
        parts.push(e.code.replace('Key', '').replace('Digit', ''));
    }

    return parts.join('+');
}

// --- 事件监听 ---
// 为页面上的交互元素绑定功能

// 1. OCR 引擎下载按钮点击事件
downloadOcrBtn.addEventListener('click', async () => {
    if (isOcrDownloading) return; // 如果正在下载，则忽略点击

    // 如果已安装，向用户确认是否要覆盖
    if (isOcrInstalled) {
        const confirmed = await confirm('本地已存在识别引擎，确定要重新下载覆盖吗？', { title: '确认重新下载', type: 'warning' });
        if (!confirmed) return;
    }

    // 更新 UI 为下载状态
    isOcrDownloading = true;
    downloadOcrBtn.disabled = true;
    downloadOcrBtn.textContent = "正在连接...";
    ocrProgressContainer.style.display = 'block';
    ocrProgressBar.value = 0;
    ocrProgressLabel.textContent = "初始化...";
    console.log("[前端] UI 已更新为下载状态, 调用后端 download_ocr...");

    try {
        await invoke('download_ocr');
    } catch (e) {
        console.error("[前端] 后端 download_ocr 调用失败:", e);
        await message(`下载失败: ${e}`, { title: '错误', type: 'error' });
        // 下载失败后，重置 UI 状态
        isOcrDownloading = false;
        downloadOcrBtn.disabled = false;
        updateOcrUI();
        ocrProgressContainer.style.display = 'none';
        console.log("[前端] 下载错误处理完成, UI已重置.");
    }
});

// 2. 翻译引擎下载按钮点击事件
downloadBtn.addEventListener('click', async () => {
    if (isTranslatorDownloading) return;

    if (isTranslatorInstalled) {
        const confirmed = await confirm('本地已存在翻译引擎，确定要重新下载覆盖吗？', { title: '确认重新下载', type: 'warning' });
        if (!confirmed) return;
    }

    isTranslatorDownloading = true;
    downloadBtn.disabled = true;
    downloadBtn.textContent = "正在连接...";
    progressContainer.style.display = 'block';
    progressBar.value = 0;
    progressLabel.textContent = "初始化...";

    try {
        await invoke('download_translator');
    } catch (e) {
        console.error("翻译引擎下载出错:", e);
        await message(`下载失败: ${e}`, { title: '错误', type: 'error' });
        isTranslatorDownloading = false;
        downloadBtn.disabled = false;
        updateTranslatorUI();
        progressContainer.style.display = 'none';
    }
});

// 3. 监听后端发送的 OCR 下载进度事件
listen('ocr-download-progress', (event) => {
    console.log("[前端] 收到 'ocr-download-progress' 事件, payload:", JSON.stringify(event.payload));
    const { progress, total, status } = event.payload;

    if (status === 'downloading') {
        const percent = Math.round((progress / total) * 100);
        ocrProgressBar.value = percent;
        const downloadedMB = (progress / 1024 / 1024).toFixed(1);
        const totalMB = (total / 1024 / 1024).toFixed(1);
        ocrProgressLabel.textContent = `正在下载... ${percent}% (${downloadedMB}MB / ${totalMB}MB)`;
    } else if (status === 'extracting') {
        ocrProgressBar.removeAttribute('value'); // 进入不确定进度状态
        ocrProgressLabel.textContent = "下载完成，正在解压安装，请稍候...";
    } else if (status === 'completed') {
        ocrProgressBar.value = 100;
        ocrProgressLabel.textContent = "安装完成！";
        isOcrDownloading = false;
        isOcrInstalled = true;
        downloadOcrBtn.disabled = false;
        updateOcrUI();
        console.log("[前端] OCR 引擎安装完成.");

        setTimeout(() => { ocrProgressContainer.style.display = 'none'; }, 2000);
    }
});


// 4. 监听后端发送的翻译引擎下载进度事件
listen('download-progress', (event) => {
    const { progress, total, status } = event.payload;

    if (status === 'downloading') {
        const percent = Math.round((progress / total) * 100);
        progressBar.value = percent;
        const downloadedMB = (progress / 1024 / 1024).toFixed(1);
        const totalMB = (total / 1024 / 1024).toFixed(1);
        progressLabel.textContent = `正在下载... ${percent}% (${downloadedMB}MB / ${totalMB}MB)`;
    } else if (status === 'extracting') {
        progressBar.removeAttribute('value');
        progressLabel.textContent = "下载完成，正在解压安装，请稍候...";
    } else if (status === 'completed') {
        progressBar.value = 100;
        progressLabel.textContent = "安装完成！";
        isTranslatorDownloading = false;
        isTranslatorInstalled = true;
        downloadBtn.disabled = false;
        updateTranslatorUI();

        setTimeout(() => { progressContainer.style.display = 'none'; }, 2000);
    }
});

// 5. 为所有设置控件绑定 'change' 事件，任何变动都立即保存
for (const radio of radioInputs) {
    radio.addEventListener('change', (e) => {
        if (e.target.checked) {
            updateUIBasedOnAction(e.target.value);
            saveSettings();
        }
    });
}
targetLangSelect.addEventListener('change', saveSettings);
lineBreakCheckbox.addEventListener('change', saveSettings);

// 6. 快捷键输入框的交互逻辑
shortcutInput.addEventListener('focus', () => {
    isRecording.main = true;
    shortcutInput.value = '请按下快捷键...';
});
shortcutInput.addEventListener('blur', () => {
    isRecording.main = false;
    // 如果用户未输入就失去焦点，恢复之前的值
    if (shortcutInput.value === '请按下快捷键...') {
        shortcutInput.value = currentSettings.shortcut || 'F1';
    }
    saveSettings(); // 保存最终结果
});
shortcutInput.addEventListener('keydown', (e) => {
    if (isRecording.main) {
        e.preventDefault(); // 阻止默认按键行为，如F1弹出帮助
        const formatted = formatShortcut(e);
        // 只接受有效的快捷键组合（带修饰键或功能键）
        if (formatted && (formatted.includes('+') || formatted.startsWith('F'))) {
            shortcutInput.value = formatted;
            shortcutInput.blur(); // 录制成功后自动失焦
        }
    }
});

// (与上面类似) 查看快捷键输入框的交互逻辑
viewShortcutInput.addEventListener('focus', () => {
    isRecording.view = true;
    viewShortcutInput.value = '请按下快捷键...';
});
viewShortcutInput.addEventListener('blur', () => {
    isRecording.view = false;
    if (viewShortcutInput.value === '请按下快捷键...') {
        viewShortcutInput.value = currentSettings.view_image_shortcut || 'F3';
    }
    saveSettings();
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

/**
 * 页面加载后执行的初始化函数。
 * 它会并行地执行所有启动任务，以提高加载速度。
 * @async
 */
async function initialize() {
    console.log("前端 main.js 开始初始化...");
    // 使用 Promise.all 并行执行多个异步的初始化任务，可以加快启动速度
    await Promise.all([
        loadSettings(),          // 从后端加载并应用设置
        checkOcrStatus(),        // 检查 OCR 引擎状态
        checkTranslatorStatus()  // 检查翻译引擎状态
    ]);
    console.log("前端初始化完成。");
}


// --- 【核心修改】启动逻辑 ---
// 之前是直接调用 initialize()，可能会在后端准备好之前就执行，导致配置加载不正确。
// 现在我们监听 `DOMContentLoaded` 事件，确保在整个页面的 HTML 结构完全加载并解析完毕后，
// 才开始执行初始化函数。这给了后端充分的准备时间，从而解决了启动时的竞态条件问题。
document.addEventListener('DOMContentLoaded', () => {
    initialize();
});