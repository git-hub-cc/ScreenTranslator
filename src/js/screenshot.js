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

// --- 函数定义 (无修改) ---

function setupCanvas(screenshotDataUrl) {
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    if (!screenshotDataUrl || typeof screenshotDataUrl !== 'string') {
        console.error("接收到的截图数据无效。");
        alert("未能加载截图数据，窗口将关闭。");
        // --- 核心修复 1: 此处是错误处理，依然用 close ---
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
        // --- 核心修复 1: 此处是错误处理，依然用 close ---
        appWindow.close();
    };
    screenCapture.src = screenshotDataUrl;
}

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

function draw() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (screenCapture) {
        ctx.drawImage(screenCapture, 0, 0, canvas.width, canvas.height);
    }
    ctx.fillStyle = 'rgba(0, 0, 0, 0.5)';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    if (isDrawing) {
        const width = currentX - startX;
        const height = currentY - startY;
        ctx.clearRect(startX, startY, width, height);
        ctx.strokeStyle = 'rgba(97, 175, 239, 0.9)';
        ctx.lineWidth = 2;
        ctx.strokeRect(startX, startY, width, height);
    }
    drawMagnifier();
    drawSizeIndicator();
}

// --- 事件监听 (无修改) ---

canvas.addEventListener('mousedown', (e) => {
    isDrawing = true;
    startX = e.clientX;
    startY = e.clientY;
    currentX = startX;
    currentY = startY;
});

canvas.addEventListener('mousemove', (e) => {
    currentX = e.clientX;
    currentY = e.clientY;
    requestAnimationFrame(draw);
});

// 鼠标松开，完成截图
canvas.addEventListener('mouseup', async (e) => {
    if (!isDrawing) return;
    isDrawing = false;

    // --- 核心修复 2: 完成截图后，隐藏窗口而不是关闭它 ---
    await appWindow.hide();

    const x = Math.min(startX, currentX);
    const y = Math.min(startY, currentY);
    const width = Math.abs(currentX - startX);
    const height = Math.abs(currentY - startY);

    if (width < 10 || height < 10) {
        console.log("选区太小，已取消");
        // 如果选区太小，我们不需要做任何事，窗口已经隐藏了。
        // 下次快捷键会重新显示它。
        return;
    }

    console.log(`向后端发送截图区域: x=${x}, y=${y}, w=${width}, h=${height}`);
    try {
        await invoke('process_screenshot_area', { x, y, width, height });
    } catch (error) {
        console.error("调用后端 'process_screenshot_area' 指令失败:", error);
    }
    // 注意：我们不再需要在这里调用 close() 或 hide()，因为前面已经 hide() 了
});

// 键盘按下，如果按下 ESC 键则取消截图
document.addEventListener('keydown', async (e) => {
    if (e.key === 'Escape') {
        console.log("截图已取消 (ESC)");
        // --- 核心修复 3: 按下 ESC 也是隐藏窗口 ---
        await appWindow.hide();
    }
});

// --- 初始化逻辑 (无修改) ---
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