// 导入Tauri API
const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;
const { message, confirm } = window.__TAURI__.dialog;

// --- DOM 元素获取 ---
// 常规设置
const shortcutInput = document.getElementById('shortcut-input');
const viewShortcutInput = document.getElementById('view-shortcut-input');
const targetLangSelect = document.getElementById('target-lang-select');
const targetLangContainer = document.getElementById('target-lang-container');
const lineBreakCheckbox = document.getElementById('line-break-checkbox');
const ocrSettingsBlock = document.getElementById('ocr-settings-block');
const radioInputs = document.getElementsByName('primary-action');

// 新增：OCR 引擎管理元素
const ocrEngineStatusBadge = document.getElementById('ocr-engine-status');
const downloadOcrBtn = document.getElementById('download-ocr-btn');
const ocrProgressContainer = document.getElementById('ocr-progress-container');
const ocrProgressBar = document.getElementById('ocr-download-progress');
const ocrProgressLabel = document.getElementById('ocr-progress-label');

// 翻译引擎管理元素
const engineStatusBadge = document.getElementById('engine-status');
const downloadBtn = document.getElementById('download-btn');
const progressContainer = document.getElementById('progress-container');
const progressBar = document.getElementById('download-progress');
const progressLabel = document.getElementById('progress-label');

// --- 状态与默认值 ---
shortcutInput.value = 'F1';
viewShortcutInput.value = 'F3';
let isRecording = { main: false, view: false };
let currentSettings = {};
let isOcrInstalled = false;
let isTranslatorInstalled = false;
let isOcrDownloading = false;
let isTranslatorDownloading = false;

// --- 函数定义 ---

// --- OCR 引擎状态管理 ---

/**
 * 检查 OCR 引擎安装状态
 */
async function checkOcrStatus() {
    console.log("[FRONTEND] 发起 OCR 状态检查...");
    try {
        isOcrInstalled = await invoke('check_ocr_status');
        // --- 日志 ---
        console.log("[FRONTEND] 收到 OCR 状态: isOcrInstalled =", isOcrInstalled);
        updateOcrUI();
    } catch (e) {
        console.error("[FRONTEND] 检查OCR引擎状态失败:", e);
        ocrEngineStatusBadge.textContent = "检查失败";
        ocrEngineStatusBadge.className = "status-badge missing";
    }
}

/**
 * 更新 OCR 引擎状态 UI
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

// --- 翻译引擎状态管理 ---

/**
 * 检查翻译引擎安装状态
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
 * 更新翻译引擎状态 UI
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
 * 根据选择的“首要动作”动态更新UI显示状态
 */
function updateUIBasedOnAction(actionValue) {
    const requiresOcr = ['ocr', 'ocr_translate', 'preview'].includes(actionValue);
    const requiresTranslation = actionValue === 'ocr_translate';

    // 1. 控制“识别与翻译设置”区块的显示
    ocrSettingsBlock.classList.toggle('hidden', !requiresOcr);

    // 2. 控制“目标语言”选项的显示
    targetLangContainer.classList.toggle('hidden', !requiresTranslation);

    // 3. 如果用户选择了需要引擎的功能但未安装，给予提示
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
 * 从后端加载设置并更新UI
 */
async function loadSettings() {
    try {
        const settings = await invoke('get_settings');
        currentSettings = settings;

        shortcutInput.value = settings.shortcut;
        viewShortcutInput.value = settings.view_image_shortcut;
        targetLangSelect.value = settings.target_lang;
        lineBreakCheckbox.checked = settings.preserve_line_breaks;

        for (const radio of radioInputs) {
            if (radio.value === settings.primary_action) {
                radio.checked = true;
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
 */
async function saveSettings() {
    const shortcutValue = shortcutInput.value.trim();
    if (!shortcutValue) {
        shortcutInput.value = currentSettings.shortcut || 'F1';
        return;
    }
    const viewShortcutValue = viewShortcutInput.value.trim();
    if (!viewShortcutValue) {
        viewShortcutInput.value = currentSettings.view_image_shortcut || 'F3';
        return;
    }

    let selectedAction = 'ocr';
    for (const radio of radioInputs) {
        if (radio.checked) {
            selectedAction = radio.value;
            break;
        }
    }

    const newSettings = {
        shortcut: shortcutValue,
        view_image_shortcut: viewShortcutValue,
        target_lang: targetLangSelect.value,
        preserve_line_breaks: lineBreakCheckbox.checked,
        primary_action: selectedAction,
    };

    try {
        await invoke('set_settings', { settings: newSettings });
        currentSettings = newSettings;
    } catch (error) {
        console.error("保存设置失败:", error);
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

// --- 事件监听 ---

// 1. OCR 引擎下载按钮逻辑
downloadOcrBtn.addEventListener('click', async () => {
    if (isOcrDownloading) {
        console.log("[FRONTEND] OCR 正在下载中，忽略点击.");
        return;
    }
    console.log("[FRONTEND] OCR 下载按钮被点击.");

    if (isOcrInstalled) {
        const confirmed = await confirm('本地已存在识别引擎，确定要重新下载覆盖吗？', { title: '确认重新下载', type: 'warning' });
        if (!confirmed) {
            console.log("[FRONTEND] 用户取消重新下载.");
            return;
        }
        console.log("[FRONTEND] 用户确认重新下载.");
    }

    isOcrDownloading = true;
    downloadOcrBtn.disabled = true;
    downloadOcrBtn.textContent = "正在连接...";
    ocrProgressContainer.style.display = 'block';
    ocrProgressBar.value = 0;
    ocrProgressLabel.textContent = "初始化...";
    console.log("[FRONTEND] UI 已更新为下载状态, 调用后端 download_ocr...");


    try {
        await invoke('download_ocr');
    } catch (e) {
        console.error("[FRONTEND] 后端 download_ocr 调用失败:", e);
        await message(`下载失败: ${e}`, { title: '错误', type: 'error' });
        isOcrDownloading = false;
        downloadOcrBtn.disabled = false;
        updateOcrUI();
        ocrProgressContainer.style.display = 'none';
        console.log("[FRONTEND] 下载错误处理完成, UI已重置.");
    }
});

// 2. 翻译引擎下载按钮逻辑
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

// 3. 监听 OCR 下载进度
listen('ocr-download-progress', (event) => {
    // --- 日志 ---
    console.log("[FRONTEND] 收到 'ocr-download-progress' 事件, payload:", JSON.stringify(event.payload));
    const { progress, total, status } = event.payload;

    if (status === 'downloading') {
        const percent = Math.round((progress / total) * 100);
        ocrProgressBar.value = percent;
        const downloadedMB = (progress / 1024 / 1024).toFixed(1);
        const totalMB = (total / 1024 / 1024).toFixed(1);
        ocrProgressLabel.textContent = `正在下载... ${percent}% (${downloadedMB}MB / ${totalMB}MB)`;
    } else if (status === 'extracting') {
        ocrProgressBar.removeAttribute('value');
        ocrProgressLabel.textContent = "下载完成，正在解压安装，请稍候...";
    } else if (status === 'completed') {
        ocrProgressBar.value = 100;
        ocrProgressLabel.textContent = "安装完成！";
        isOcrDownloading = false;
        isOcrInstalled = true;
        downloadOcrBtn.disabled = false;
        updateOcrUI();
        console.log("[FRONTEND] OCR 引擎安装完成.");

        setTimeout(() => {
            ocrProgressContainer.style.display = 'none';
        }, 2000);
    }
});


// 4. 监听翻译下载进度
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

        setTimeout(() => {
            progressContainer.style.display = 'none';
        }, 2000);
    }
});

// 5. 常规设置控件事件
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

// 6. 快捷键输入逻辑
shortcutInput.addEventListener('focus', () => {
    isRecording.main = true;
    shortcutInput.value = '请按下快捷键...';
});
shortcutInput.addEventListener('blur', () => {
    isRecording.main = false;
    if (shortcutInput.value === '请按下快捷键...') {
        shortcutInput.value = currentSettings.shortcut || 'F1';
    }
    saveSettings();
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
 */
async function initialize() {
    console.log("前端 main.js 加载完成，开始初始化...");
    // 异步并行执行，提高启动速度
    await Promise.all([
        loadSettings(),
        checkOcrStatus(),
        checkTranslatorStatus()
    ]);
    console.log("前端初始化完成。");
}

// 脚本加载后立即执行初始化
initialize();