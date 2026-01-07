// --- 文件: src/js/image_viewer.js ---

// 从 Tauri 的 API 中导入必要的模块
const { listen } = window.__TAURI__.event;
const { appWindow, primaryMonitor, PhysicalSize } = window.__TAURI__.window;
const { invoke } = window.__TAURI__.tauri;
const { isPermissionGranted, requestPermission, sendNotification } = window.__TAURI__.notification;

// --- DOM 元素获取 ---
// 预先获取页面上需要操作的 HTML 元素，提高性能并使代码更清晰
const imageEl = document.getElementById('screenshot-image');
const btnOcr = document.getElementById('btn-ocr');
const btnCopy = document.getElementById('btn-copy');
const btnSave = document.getElementById('btn-save');
const btnClose = document.getElementById('btn-close');

// --- 全局状态变量 ---
let currentImagePath = ""; // 用于存储当前显示的图片在文件系统中的路径

// --- 函数定义 ---

/**
 * 显示一个系统通知。
 * 会自动检查并请求通知权限。
 * @param {string} title - 通知标题。
 * @param {string} body - 通知正文。
 */
async function notify(title, body) {
    try {
        let permissionGranted = await isPermissionGranted();
        if (!permissionGranted) {
            const permission = await requestPermission();
            permissionGranted = permission === 'granted';
        }
        if (permissionGranted) {
            sendNotification({ title, body });
        }
    } catch (error) {
        console.error("发送通知失败:", error);
    }
}

/**
 * 根据图片的原始尺寸，动态地调整预览窗口的大小。
 * 同时会限制窗口尺寸不超过屏幕可用区域的90%，并将其居中。
 * @param {number} imgWidth - 图片的原始宽度。
 * @param {number} imgHeight - 图片的原始高度。
 */
async function resizeWindowToFitImage(imgWidth, imgHeight) {
    const monitor = await primaryMonitor();
    if (!monitor) {
        console.error("无法获取主显示器信息。");
        return;
    }

    // 为窗口边框留出一点余量
    const BORDER_WIDTH = 4;
    const newWidth = imgWidth + BORDER_WIDTH;
    const newHeight = imgHeight + BORDER_WIDTH;

    // 计算最大允许的窗口尺寸（屏幕的 90%）
    const maxWidth = monitor.size.width * 0.90;
    const maxHeight = monitor.size.height * 0.90;

    // 最终尺寸取图片实际大小和屏幕限制中的较小值
    const finalWidth = Math.min(newWidth, maxWidth);
    const finalHeight = Math.min(newHeight, maxHeight);

    await appWindow.setSize(new PhysicalSize(finalWidth, finalHeight));
    await appWindow.center();
}

// --- 事件监听 ---

// 1. 监听由 Rust 后端发送的 'display-image' 事件
listen('display-image', async (event) => {
    const payload = event.payload;
    if (!payload || !payload.image_data_url || !payload.image_path) {
        console.error("接收到的图片数据无效:", payload);
        return;
    }
    console.log("预览窗口接收到图片数据:", payload);

    // 更新当前图片路径
    currentImagePath = payload.image_path;

    // 使用一个临时的 Image 对象来预加载图片，以便在显示前获取其原始尺寸
    const tempImg = new Image();
    tempImg.onload = async () => {
        // 根据图片尺寸调整窗口大小
        await resizeWindowToFitImage(tempImg.naturalWidth, tempImg.naturalHeight);
        // 设置图片源并显示窗口
        imageEl.src = payload.image_data_url;
        await appWindow.show();
        await appWindow.setFocus(); // 确保窗口获得焦点
    };
    tempImg.onerror = (err) => {
        console.error("预加载图片失败:", err);
    };
    tempImg.src = payload.image_data_url;
});

// 2. 监听鼠标按下事件，实现窗口拖拽
document.body.addEventListener('mousedown', (e) => {
    // 确保只有在点击非按钮区域时才触发拖拽，以防止按钮点击失效
    if (e.target.tagName !== 'BUTTON') {
        appWindow.startDragging();
    }
});

// 3. 监听键盘事件，实现 ESC 键关闭窗口
document.addEventListener('keydown', async (e) => {
    if (e.key === 'Escape') {
        await appWindow.hide();
    }
});

// 4. 禁用右键上下文菜单
// 为了保持界面简洁并防止意外操作，我们禁用整个页面的默认右键菜单。
document.addEventListener('contextmenu', (event) => {
    event.preventDefault(); // 调用 preventDefault() 可以阻止默认行为（即弹出菜单）
});


// --- 工具栏按钮事件 ---

// 按钮：识别文字 (OCR)
btnOcr.addEventListener('click', async () => {
    if (!currentImagePath) return;
    await appWindow.hide(); // 隐藏当前窗口，避免遮挡可能出现的结果窗口

    try {
        // --- 核心修改：将动作从 'ocr' (仅识别) 改为 'ocr_translate' (识别并翻译) ---
        // 这样，后端就会执行完整的“识别+翻译”流程，并弹出结果窗口。
        await invoke('process_image_from_path', {
            path: currentImagePath,
            action: 'ocr_translate'
        });
    } catch (err) {
        console.error("手动触发OCR失败:", err);
        await notify("错误", `处理失败: ${err}`);
    }
});

// 按钮：复制图片
btnCopy.addEventListener('click', async () => {
    if (!currentImagePath) return;
    try {
        await invoke('copy_image_to_clipboard', { path: currentImagePath });
        await notify('复制成功', '图片已复制到剪贴板');
    } catch (err) {
        console.error("复制图片失败:", err);
        await notify('复制失败', err);
    }
});

// 按钮：另存图片
btnSave.addEventListener('click', async () => {
    if (!currentImagePath) return;
    try {
        await invoke('save_image_to_desktop', { path: currentImagePath });
        await notify('保存成功', '图片已保存到桌面');
    } catch (err) {
        console.error("保存图片失败:", err);
        await notify('保存失败', err);
    }
});

// 按钮：关闭窗口
btnClose.addEventListener('click', async () => {
    await appWindow.hide();
});