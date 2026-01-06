// 从 tauri APIs 中导入所需模块
const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;
const { appWindow } = window.__TAURI__.window;
const { writeText } = window.__TAURI__.clipboard;
const { isPermissionGranted, requestPermission, sendNotification } = window.__TAURI__.notification;

// --- DOM 元素获取 ---
const originalTextEl = document.getElementById('original-text');
const translatedTextEl = document.getElementById('translated-text');
const pinBtn = document.getElementById('pin-btn');
const copyOriginalBtn = document.getElementById('copy-original-btn');
const copyTranslatedBtn = document.getElementById('copy-translated-btn');
const copyImageBtn = document.getElementById('copy-image-btn');
const saveImageBtn = document.getElementById('save-image-btn');
const ttsBtn = document.getElementById('tts-btn');

// --- 状态变量 ---
let isPinned = true;
let originalTextContent = '';
let translatedTextContent = '';
let currentImagePath = '';

// --- 函数定义 (保持不变) ---

function giveFeedback(element) {
    if (!element) return;
    element.classList.add('clicked-feedback');
    setTimeout(() => {
        element.classList.remove('clicked-feedback');
    }, 200);
}

async function notify(title, body) {
    let permissionGranted = await isPermissionGranted();
    if (!permissionGranted) {
        const permission = await requestPermission();
        permissionGranted = permission === 'granted';
    }
    if (permissionGranted) {
        sendNotification({ title, body });
    }
}

async function togglePin() {
    isPinned = !isPinned;
    await appWindow.setAlwaysOnTop(isPinned);
    pinBtn.classList.toggle('active', isPinned);
    pinBtn.title = isPinned ? "取消置顶" : "钉在最前";
}

async function copyText(text, type) {
    if (!text) return;
    try {
        await writeText(text);
        await notify('复制成功', `${type}已复制到剪贴板。`);
    } catch (error) {
        console.error(`复制 ${type} 失败:`, error);
        await notify('复制失败', `未能将${type}复制到剪贴板。`);
    }
}

function speakText() {
    if (!translatedTextContent || window.speechSynthesis.speaking) return;
    const utterance = new SpeechSynthesisUtterance(translatedTextContent);
    window.speechSynthesis.speak(utterance);
}

// --- 事件监听 (核心修改) ---

/**
 * 监听 OCR 结果事件。
 * 这是收到的第一个事件，用于立即更新原文和图片信息。
 */
listen('ocr_result', (event) => {
    // --- 日志 ---
    console.log("[RESULTS.JS] 接收到 'ocr_result' 事件, payload:", event.payload);

    const payload = event.payload;

    // 重置UI状态
    translatedTextEl.style.color = 'var(--text-color-bright)';

    // 步骤 1: 处理图片路径，无论成功失败都执行
    if (payload.image_path) {
        currentImagePath = payload.image_path;
        copyImageBtn.style.display = 'inline-block';
        saveImageBtn.style.display = 'inline-block';
    } else {
        currentImagePath = '';
        copyImageBtn.style.display = 'none';
        saveImageBtn.style.display = 'none';
    }

    // 步骤 2: 根据是否有错误来更新文本区域
    if (payload.error_message) {
        // OCR 阶段就发生错误
        originalTextContent = payload.original_text || "错误";
        translatedTextContent = payload.error_message;

        originalTextEl.textContent = originalTextContent;
        translatedTextEl.textContent = translatedTextContent;
        translatedTextEl.style.color = 'var(--error-color)';
    } else {
        // OCR 成功
        originalTextContent = payload.original_text || '';
        translatedTextContent = ''; // 清空，等待翻译结果

        originalTextEl.textContent = originalTextContent;
        translatedTextEl.textContent = '翻译中...'; // 关键的用户反馈
    }
});


/**
 * 监听翻译更新事件。
 * 这个事件在 OCR 成功后才会收到，用于更新译文区域。
 */
listen('translation_update', (event) => {
    // --- 日志 ---
    console.log("[RESULTS.JS] 接收到 'translation_update' 事件, payload:", event.payload);

    const payload = event.payload;

    // 根据翻译结果更新UI
    if (payload.error_message) {
        // 翻译阶段发生错误
        translatedTextContent = payload.error_message;
        translatedTextEl.textContent = translatedTextContent;
        translatedTextEl.style.color = 'var(--error-color)';
    } else {
        // 翻译成功
        translatedTextContent = payload.translated_text || '';
        translatedTextEl.textContent = translatedTextContent;
        translatedTextEl.style.color = 'var(--text-color-bright)';
    }
});


// --- 其他交互事件 (保持不变) ---
document.body.addEventListener('dblclick', () => {
    appWindow.close();
});

document.addEventListener('keydown', async (e) => {
    if (e.key === 'Escape') {
        await appWindow.close();
    }
});

pinBtn.addEventListener('click', togglePin);

copyOriginalBtn.addEventListener('click', () => {
    giveFeedback(copyOriginalBtn);
    copyText(originalTextContent, '原文');
});

copyTranslatedBtn.addEventListener('click', () => {
    giveFeedback(copyTranslatedBtn);
    copyText(translatedTextContent, '译文');
});

ttsBtn.addEventListener('click', () => {
    giveFeedback(ttsBtn);
    speakText();
});

copyImageBtn.addEventListener('click', async () => {
    giveFeedback(copyImageBtn);
    if (!currentImagePath) return;
    try {
        await invoke('copy_image_to_clipboard', { path: currentImagePath });
        await notify('复制成功', '截图已复制到剪贴板。');
    } catch (error) {
        console.error("复制图片失败:", error);
        await notify('复制失败', `错误: ${error}`);
    }
});

saveImageBtn.addEventListener('click', async () => {
    giveFeedback(saveImageBtn);
    if (!currentImagePath) return;
    try {
        await invoke('save_image_to_desktop', { path: currentImagePath });
        await notify('保存成功', '截图已保存到桌面。');
    } catch (error) {
        console.error("保存图片失败:", error);
        await notify('保存失败', `错误: ${error}`);
    }
});

// --- 初始化 ---
pinBtn.classList.add('active');
pinBtn.title = "取消置顶";
copyImageBtn.style.display = 'none';
saveImageBtn.style.display = 'none';

console.log("[RESULTS.JS] 结果窗口初始化完成，事件监听器已设置。");