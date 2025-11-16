// 从 tauri APIs 中导入所需模块
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
const ttsBtn = document.getElementById('tts-btn');

// --- 状态变量 ---
let isPinned = true; // 窗口默认就是置顶的
let originalTextContent = '';
let translatedTextContent = '';

// --- 函数定义 ---

/**
 * 显示系统通知
 * @param {string} title - 通知标题
 * @param {string} body - 通知内容
 */
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

/**
 * 切换窗口的置顶状态
 */
async function togglePin() {
    isPinned = !isPinned;
    await appWindow.setAlwaysOnTop(isPinned);
    pinBtn.classList.toggle('active', isPinned);
    pinBtn.title = isPinned ? "取消置顶" : "钉在最前";
}

/**
 * 复制文本到剪贴板
 * @param {string} text - 要复制的文本
 * @param {string} type - 文本类型 (e.g., "原文", "译文")
 */
async function copyText(text, type) {
    if (!text) return;
    await writeText(text);
    await notify('复制成功', `${type}已复制到剪贴板。`);
}

/**
 * 使用浏览器TTS API朗读文本
 */
function speakText() {
    if (!translatedTextContent || window.speechSynthesis.speaking) return;
    const utterance = new SpeechSynthesisUtterance(translatedTextContent);
    // 可以在这里设置语言等
    // utterance.lang = 'zh-CN';
    window.speechSynthesis.speak(utterance);
}


// --- 事件监听 ---

// 监听由 Rust 后端发出的 "translation_result" 事件
listen('translation_result', (event) => {
    const payload = event.payload;
    console.log("接收到翻译结果:", payload);

    if (payload.error_message) {
        originalTextContent = "错误";
        translatedTextContent = payload.error_message;
        originalTextEl.textContent = originalTextContent;
        translatedTextEl.textContent = translatedTextContent;
        translatedTextEl.style.color = 'var(--error-color)';
    } else {
        originalTextContent = payload.original_text;
        translatedTextContent = payload.translated_text;
        originalTextEl.textContent = originalTextContent;
        translatedTextEl.textContent = translatedTextContent;
        translatedTextEl.style.color = 'var(--text-color-bright)';
    }
});

// 点击窗口外部（或body）关闭窗口
// 为了允许用户复制文本，我们将关闭事件改为双击
document.body.addEventListener('dblclick', () => {
    appWindow.close();
});

// 按下 Esc 键关闭窗口
document.addEventListener('keydown', async (e) => {
    if (e.key === 'Escape') {
        await appWindow.close();
    }
});

// --- 工具栏按钮事件 ---
pinBtn.addEventListener('click', togglePin);
copyOriginalBtn.addEventListener('click', () => copyText(originalTextContent, '原文'));
copyTranslatedBtn.addEventListener('click', () => copyText(translatedTextContent, '译文'));
ttsBtn.addEventListener('click', speakText);


// --- 初始化 ---
// 确保初始状态正确
pinBtn.classList.add('active');
pinBtn.title = "取消置顶";