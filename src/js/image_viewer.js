const { listen } = window.__TAURI__.event;
const { appWindow, primaryMonitor, PhysicalSize } = window.__TAURI__.window;
const { invoke } = window.__TAURI__.tauri;
const { isPermissionGranted, requestPermission, sendNotification } = window.__TAURI__.notification;

// --- DOM 元素获取 ---
const imageEl = document.getElementById('screenshot-image');

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
 * 根据图片大小动态调整窗口尺寸
 * @param {number} imgWidth - 图片的原始宽度
 * @param {number} imgHeight - 图片的原始高度
 */
async function resizeWindowToFitImage(imgWidth, imgHeight) {
    const monitor = await primaryMonitor();
    if (!monitor) return;

    // 加上边框的额外空间 (2px * 2)
    const BORDER_WIDTH = 4;
    const newWidth = imgWidth + BORDER_WIDTH;
    const newHeight = imgHeight + BORDER_WIDTH;

    // 限制窗口尺寸不超过屏幕可用空间的95%
    const maxWidth = monitor.size.width * 0.95;
    const maxHeight = monitor.size.height * 0.95;

    const finalWidth = Math.min(newWidth, maxWidth);
    const finalHeight = Math.min(newHeight, maxHeight);

    await appWindow.setSize(new PhysicalSize(finalWidth, finalHeight));
    await appWindow.center();
}

// --- 事件监听 ---

// 监听由 Rust 后端发出的 "display-image" 事件
listen('display-image', async (event) => {
    const payload = event.payload;
    console.log("接收到图片数据:", payload);

    const tempImg = new Image();
    tempImg.onload = async () => {
        await resizeWindowToFitImage(tempImg.naturalWidth, tempImg.naturalHeight);
        imageEl.src = payload.image_data_url;
        await appWindow.show();
        await appWindow.set_focus(); // 使用 set_focus 保证跨平台兼容性
    };
    tempImg.src = payload.image_data_url;
});

// 按下 Esc 键隐藏窗口
document.addEventListener('keydown', async (e) => {
    if (e.key === 'Escape') {
        // --- 核心修复 3: 从 close() 改为 hide() ---
        await appWindow.hide();
    }
});

// 鼠标双击图片隐藏窗口
imageEl.addEventListener('dblclick', async () => {
    // --- 核心修复 3: 从 close() 改为 hide() ---
    await appWindow.hide();
});

// --- 新增：为整个窗口添加拖拽功能 ---
document.body.addEventListener('mousedown', () => {
    // 调用 Tauri API 开始拖动窗口
    appWindow.startDragging();
});