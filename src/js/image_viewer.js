const { listen } = window.__TAURI__.event;
const { appWindow, primaryMonitor, PhysicalSize } = window.__TAURI__.window;
const { invoke } = window.__TAURI__.tauri;
const { isPermissionGranted, requestPermission, sendNotification } = window.__TAURI__.notification;

// --- DOM 元素获取 ---
const imageEl = document.getElementById('screenshot-image');
const btnOcr = document.getElementById('btn-ocr');
const btnCopy = document.getElementById('btn-copy');
const btnSave = document.getElementById('btn-save');
const btnClose = document.getElementById('btn-close');

let currentImagePath = "";

// --- 函数定义 ---

/**
 * 显示系统通知
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
 */
async function resizeWindowToFitImage(imgWidth, imgHeight) {
    const monitor = await primaryMonitor();
    if (!monitor) return;

    const BORDER_WIDTH = 4;
    const newWidth = imgWidth + BORDER_WIDTH;
    const newHeight = imgHeight + BORDER_WIDTH;

    const maxWidth = monitor.size.width * 0.90;
    const maxHeight = monitor.size.height * 0.90;

    const finalWidth = Math.min(newWidth, maxWidth);
    const finalHeight = Math.min(newHeight, maxHeight);

    await appWindow.setSize(new PhysicalSize(finalWidth, finalHeight));
    await appWindow.center();
}

// --- 事件监听 ---

// 1. 监听后端发送的图片展示事件
listen('display-image', async (event) => {
    const payload = event.payload;
    console.log("预览窗口接收到图片数据:", payload);

    currentImagePath = payload.image_path;

    const tempImg = new Image();
    tempImg.onload = async () => {
        await resizeWindowToFitImage(tempImg.naturalWidth, tempImg.naturalHeight);
        imageEl.src = payload.image_data_url;
        await appWindow.show();
        await appWindow.set_focus();
    };
    tempImg.src = payload.image_data_url;
});

// 2. 窗口拖拽
document.body.addEventListener('mousedown', (e) => {
    // 只有点击非按钮区域才触发拖拽
    if(e.target.tagName !== 'BUTTON') {
        appWindow.startDragging();
    }
});

// 3. 按 ESC 隐藏窗口
document.addEventListener('keydown', async (e) => {
    if (e.key === 'Escape') {
        await appWindow.hide();
    }
});

// --- 工具栏按钮事件 ---

// 按钮：识别文字 (调用后端手动处理命令，指定模式为 OCR)
btnOcr.addEventListener('click', async () => {
    if (!currentImagePath) return;

    // 关闭当前预览窗口，以免干扰
    await appWindow.hide();

    // 通知后端对当前图片路径执行“OCR”模式的处理
    // 我们复用后端的逻辑，但传入一个临时的覆盖设置
    try {
        await invoke('process_image_from_path', {
            path: currentImagePath,
            action: 'ocr' // 强制执行OCR模式，会弹出结果窗口或复制
        });
    } catch (err) {
        console.error("手动触发OCR失败:", err);
        notify("错误", `OCR失败: ${err}`);
    }
});

// 按钮：复制图片
btnCopy.addEventListener('click', async () => {
    if (!currentImagePath) return;
    try {
        await invoke('copy_image_to_clipboard', { path: currentImagePath });
        notify('复制成功', '图片已复制到剪贴板');
        // 可选：复制后自动关闭窗口
        // await appWindow.hide();
    } catch (err) {
        console.error("复制失败:", err);
        notify('复制失败', err);
    }
});

// 按钮：保存图片
btnSave.addEventListener('click', async () => {
    if (!currentImagePath) return;
    try {
        await invoke('save_image_to_desktop', { path: currentImagePath });
        notify('保存成功', '图片已保存到桌面');
    } catch (err) {
        console.error("保存失败:", err);
        notify('保存失败', err);
    }
});

// 按钮：关闭
btnClose.addEventListener('click', async () => {
    await appWindow.hide();
});