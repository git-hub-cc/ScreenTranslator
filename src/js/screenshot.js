// 从 tauri APIs 中导入所需模块
const { invoke } = window.__TAURI__.tauri;
const { appWindow } = window.__TAURI__.window;

// --- DOM 元素获取 ---
const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');

// --- 状态变量定义 ---
let isDrawing = false; // 标记是否正在拖拽鼠标
let startX, startY; // 截图起始点坐标
let currentX, currentY; // 鼠标当前位置坐标

// 用于暂存整个屏幕的截图，以实现放大镜效果
let screenCapture = null;

// --- 函数定义 ---

/**
 * 调整 canvas 尺寸以匹配窗口大小，并获取全屏截图
 */
async function setupCanvas() {
    // 调整尺寸
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    // 获取整个屏幕的截图数据URL
    // 注意：这里的实现依赖于一个假设的Tauri API `appWindow.captureScreen()`
    // 实际项目中可能需要通过后端实现截图并传给前端
    // 为简化MVP，我们先用一个黑色背景模拟

    // 真实的实现应该是这样的：
    // const screenshotDataUrl = await invoke('take_fullscreen_screenshot');
    // screenCapture = new Image();
    // screenCapture.src = screenshotDataUrl;
    // screenCapture.onload = () => {
    //    draw();
    // };

    // 模拟实现
    draw();
}

/**
 * 绘制放大镜
 */
function drawMagnifier() {
    if (!isDrawing) return; // 仅在非拖拽状态下显示

    const magnifierSize = 120; // 放大镜的尺寸
    const zoomFactor = 2; // 放大倍数

    // 放大镜显示位置（右上角）
    const magnifierX = canvas.width - magnifierSize - 20;
    const magnifierY = 20;

    // 绘制放大镜背景和边框
    ctx.save();
    ctx.globalAlpha = 1; // 确保放大镜不透明
    ctx.fillStyle = '#000';
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.7)';
    ctx.lineWidth = 2;
    ctx.fillRect(magnifierX, magnifierY, magnifierSize, magnifierSize);
    ctx.strokeRect(magnifierX, magnifierY, magnifierSize, magnifierSize);

    // 设置剪切区域，防止绘制内容超出放大镜范围
    ctx.beginPath();
    ctx.rect(magnifierX, magnifierY, magnifierSize, magnifierSize);
    ctx.clip();

    // 绘制被放大的屏幕内容
    // 从鼠标当前位置的左上方开始取源图像
    const sourceX = currentX - (magnifierSize / zoomFactor / 2);
    const sourceY = currentY - (magnifierSize / zoomFactor / 2);
    const sourceWidth = magnifierSize / zoomFactor;
    const sourceHeight = magnifierSize / zoomFactor;

    // 这里因为没有真实的 screenCapture, 我们无法绘制
    // 如果有 screenCapture, 代码会是这样：
    // ctx.drawImage(screenCapture,
    //               sourceX, sourceY, sourceWidth, sourceHeight,
    //               magnifierX, magnifierY, magnifierSize, magnifierSize);

    // 绘制十字准星
    ctx.strokeStyle = '#ff0000';
    ctx.lineWidth = 1;
    // 水平线
    ctx.beginPath();
    ctx.moveTo(magnifierX, magnifierY + magnifierSize / 2);
    ctx.lineTo(magnifierX + magnifierSize, magnifierY + magnifierSize / 2);
    ctx.stroke();
    // 垂直线
    ctx.beginPath();
    ctx.moveTo(magnifierX + magnifierSize / 2, magnifierY);
    ctx.lineTo(magnifierX + magnifierSize / 2, magnifierY + magnifierSize);
    ctx.stroke();

    ctx.restore();
}

/**
 * 绘制尺寸提示
 */
function drawSizeIndicator() {
    if (!isDrawing) return;

    const width = Math.abs(currentX - startX);
    const height = Math.abs(currentY - startY);
    const text = `${width} x ${height}`;

    const textX = Math.min(startX, currentX) + width + 10;
    const textY = Math.min(startY, currentY) + height + 20;

    ctx.fillStyle = 'rgba(0, 0, 0, 0.7)';
    ctx.fillRect(textX - 5, textY - 15, ctx.measureText(text).width + 10, 20);
    ctx.fillStyle = '#fff';
    ctx.font = '14px Arial';
    ctx.fillText(text, textX, textY);
}


/**
 * 绘制整个截图界面
 */
function draw() {
    // 1. 清空画布
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // 2. 绘制半透明的灰色蒙版
    ctx.fillStyle = 'rgba(0, 0, 0, 0.5)';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // 3. 如果正在拖拽，高亮选区
    if (isDrawing) {
        const width = currentX - startX;
        const height = currentY - startY;

        // 清除选区内的蒙版
        ctx.clearRect(startX, startY, width, height);
        // 绘制选区边框
        ctx.strokeStyle = 'rgba(97, 175, 239, 0.9)'; // --accent-color
        ctx.lineWidth = 2;
        ctx.strokeRect(startX, startY, width, height);
    }

    // 4. 绘制高级UI
    // drawMagnifier(); // 暂时注释，因为缺少全屏截图数据
    drawSizeIndicator();
}

// --- 事件监听 ---

// 鼠标按下，开始截图
canvas.addEventListener('mousedown', (e) => {
    isDrawing = true;
    startX = e.clientX;
    startY = e.clientY;
    currentX = startX;
    currentY = startY;
});

// 鼠标移动，更新选区和UI
canvas.addEventListener('mousemove', (e) => {
    currentX = e.clientX;
    currentY = e.clientY;
    requestAnimationFrame(draw);
});

// 鼠标松开，完成截图
canvas.addEventListener('mouseup', async (e) => {
    if (!isDrawing) return;
    isDrawing = false;

    await appWindow.hide();

    const x = Math.min(startX, currentX);
    const y = Math.min(startY, currentY);
    const width = Math.abs(currentX - startX);
    const height = Math.abs(currentY - startY);

    if (width < 10 || height < 10) {
        console.log("选区太小，已取消");
        await appWindow.close();
        return;
    }

    console.log(`向后端发送截图区域: x=${x}, y=${y}, w=${width}, h=${height}`);
    try {
        await invoke('process_screenshot_area', { x, y, width, height });
    } catch (error) {
        console.error("调用后端指令失败:", error);
    } finally {
        await appWindow.close();
    }
});

// 键盘按下，ESC取消
document.addEventListener('keydown', async (e) => {
    if (e.key === 'Escape') {
        console.log("截图已取消 (ESC)");
        await appWindow.close();
    }
});

// --- 初始化 ---
setupCanvas();