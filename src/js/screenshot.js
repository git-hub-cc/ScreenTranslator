// 从 tauri APIs 中导入所需模块
const { invoke } = window.__TAURI__.tauri;
const { appWindow } = window.__TAURI__.window;
const { listen } = window.__TAURI__.event;

// --- DOM 元素获取 ---
const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');

// --- 状态变量定义 ---
let isDrawing = false;
let startX, startY;
let currentX, currentY;
let screenCapture = null;

// --- 函数定义 ---

/**
 * 设置画布，加载并显示全屏背景图。
 * @param {string} screenshotDataUrl - 后端传递的包含全屏截图的 Base64 Data URL。
 */
function setupCanvas(screenshotDataUrl) {
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    if (!screenshotDataUrl || typeof screenshotDataUrl !== 'string') {
        console.error("接收到的截图数据无效。");
        alert("未能加载截图数据，窗口将关闭。");
        appWindow.close();
        return;
    }

    screenCapture = new Image();
    screenCapture.onload = () => {
        console.log("全屏截图加载完成，开始绘制界面。");
        draw();
    };
    screenCapture.onerror = (err) => {
        console.error("加载截图数据URL失败:", err);
        alert("无法加载截图，请重试。");
        appWindow.close();
    };
    screenCapture.src = screenshotDataUrl;
}

/**
 * 绘制鼠标指针位置的放大镜效果。
 */
function drawMagnifier() {
    if (!screenCapture || !currentX) return;
    const magnifierSize = 120;
    const zoomFactor = 2;
    const magnifierX = canvas.width - magnifierSize - 20;
    const magnifierY = 20;
    ctx.save();
    ctx.beginPath();
    ctx.rect(magnifierX, magnifierY, magnifierSize, magnifierSize);
    ctx.clip();
    const sourceX = currentX - (magnifierSize / zoomFactor / 2);
    const sourceY = currentY - (magnifierSize / zoomFactor / 2);
    const sourceWidth = magnifierSize / zoomFactor;
    const sourceHeight = magnifierSize / zoomFactor;
    ctx.drawImage(screenCapture,
        sourceX, sourceY, sourceWidth, sourceHeight,
        magnifierX, magnifierY, magnifierSize, magnifierSize);
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.7)';
    ctx.lineWidth = 2;
    ctx.strokeRect(magnifierX, magnifierY, magnifierSize, magnifierSize);
    ctx.strokeStyle = '#ff0000';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(magnifierX, magnifierY + magnifierSize / 2);
    ctx.lineTo(magnifierX + magnifierSize, magnifierY + magnifierSize / 2);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(magnifierX + magnifierSize / 2, magnifierY);
    ctx.lineTo(magnifierX + magnifierSize / 2, magnifierY + magnifierSize);
    ctx.stroke();
    ctx.restore();
}

/**
 * 在选区右下角绘制尺寸指示器。
 */
function drawSizeIndicator() {
    if (!isDrawing) return;
    const width = Math.abs(currentX - startX);
    const height = Math.abs(currentY - startY);
    if (width === 0 || height === 0) return;
    const text = `${width} x ${height}`;
    const rectX = Math.min(startX, currentX);
    const rectY = Math.min(startY, currentY);
    let textX = rectX + width + 5;
    let textY = rectY + height + 20;
    ctx.font = '14px Arial';
    const textWidth = ctx.measureText(text).width;
    ctx.fillStyle = 'rgba(0, 0, 0, 0.7)';
    ctx.fillRect(textX - 5, textY - 15, textWidth + 10, 20);
    ctx.fillStyle = '#fff';
    ctx.fillText(text, textX, textY);
}

/**
 * 主绘制函数，每一帧都会被调用以更新画布。
 */
function draw() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (screenCapture) {
        ctx.drawImage(screenCapture, 0, 0, canvas.width, canvas.height);
    }
    // 绘制半透明蒙版
    ctx.fillStyle = 'rgba(0, 0, 0, 0.5)';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    if (isDrawing) {
        const width = currentX - startX;
        const height = currentY - startY;
        // 清除选区内的蒙版，使其高亮
        ctx.clearRect(startX, startY, width, height);
        // 绘制选区边框
        ctx.strokeStyle = 'rgba(97, 175, 239, 0.9)';
        ctx.lineWidth = 2;
        ctx.strokeRect(startX, startY, width, height);
    }
    drawMagnifier();
    drawSizeIndicator();
}

/**
 * --- 新增：封装取消截图的逻辑 ---
 * 隐藏窗口并通知后端释放截图锁。
 */
async function cancel_screenshot() {
    console.log("截图已取消，正在隐藏窗口并通知后端。");
    // 先隐藏窗口，给用户即时反馈
    await appWindow.hide();
    try {
        // 调用后端命令来释放 is_capturing 锁
        await invoke('cancel_screenshot');
    } catch (error) {
        console.error("调用后端 'cancel_screenshot' 失败:", error);
    }
}


// --- 事件监听 ---

canvas.addEventListener('mousedown', (e) => {
    // 仅响应鼠标左键
    if (e.button !== 0) return;
    isDrawing = true;
    startX = e.clientX;
    startY = e.clientY;
    currentX = startX;
    currentY = startY;
});

canvas.addEventListener('mousemove', (e) => {
    currentX = e.clientX;
    currentY = e.clientY;
    // 使用 requestAnimationFrame 来优化绘图，避免卡顿
    requestAnimationFrame(draw);
});

// 鼠标松开，完成截图
canvas.addEventListener('mouseup', async (e) => {
    // 仅响应鼠标左键
    if (e.button !== 0 || !isDrawing) return;
    isDrawing = false;

    // 完成截图后，隐藏窗口
    await appWindow.hide();

    const x = Math.min(startX, currentX);
    const y = Math.min(startY, currentY);
    const width = Math.abs(currentX - startX);
    const height = Math.abs(currentY - startY);

    // 如果选区太小，则视为取消操作，直接释放锁
    if (width < 10 || height < 10) {
        console.log("选区太小，已取消");
        // 调用封装好的取消函数来释放锁
        await cancel_screenshot();
        return;
    }

    console.log(`向后端发送截图区域: x=${x}, y=${y}, w=${width}, h=${height}`);
    try {
        // 通知后端处理截图区域，后端处理完后会自己释放锁
        await invoke('process_screenshot_area', { x, y, width, height });
    } catch (error) {
        console.error("调用后端 'process_screenshot_area' 指令失败:", error);
        // 如果调用失败，也需要确保锁被释放
        await cancel_screenshot();
    }
});

// --- 新增：监听右键点击事件以取消截图 ---
canvas.addEventListener('contextmenu', async (e) => {
    // 阻止默认的浏览器右键菜单
    e.preventDefault();
    // 如果正在绘制，则取消；如果还没开始，右击也视为取消
    console.log("截图已取消 (右键)");
    await cancel_screenshot();
});


// --- 修改：监听键盘按下事件，主要用于处理 ESC 键取消 ---
document.addEventListener('keydown', async (e) => {
    if (e.key === 'Escape') {
        console.log("截图已取消 (ESC)");
        await cancel_screenshot();
    }
});

// --- 初始化逻辑 ---
async function initialize() {
    console.log("截图窗口前端已加载，等待后端推送初始化数据...");
    const unlisten = await listen('initialize-screenshot', (event) => {
        console.log("接收到来自后端的初始化事件:", event);
        if (event.payload && event.payload.image_data_url) {
            setupCanvas(event.payload.image_data_url);
        } else {
            console.error("初始化事件的载荷无效:", event.payload);
            alert("初始化截图失败：数据错误。");
            appWindow.close();
        }
    });
}

initialize();