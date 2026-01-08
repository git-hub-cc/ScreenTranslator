// --- 文件: src/js/screenshot.js ---

// --- 从 Tauri API 中导入所需模块 ---
const { invoke } = window.__TAURI__.tauri;
const { appWindow } = window.__TAURI__.window;
const { listen } = window.__TAURI__.event;
const { writeText } = window.__TAURI__.clipboard;

// --- DOM 元素获取 ---
const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');

// --- 状态变量定义 ---
let isDrawing = false;      // 标记是否正在绘制选区
let startX, startY;         // 选区起始坐标
let currentX, currentY;     // 鼠标当前坐标
let screenCapture = null;   // 存储从后端接收的全屏截图 Image 对象

// --- 颜色拾取器相关状态 ---
let currentColor = null;    // 存储当前鼠标下方像素的颜色信息
let colorFormat = 'hex';    // 当前颜色值的显示格式 ('hex' 或 'rgb')
let copyFeedback = false;   // 标记是否显示“复制成功”的视觉反馈

// --- 辅助函数 ---

/**
 * 将 RGB 颜色分量转换为十六进制（HEX）字符串。
 * @param {number} r - 红色分量 (0-255)。
 * @param {number} g - 绿色分量 (0-255)。
 * @param {number} b - 蓝色分量 (0-255)。
 * @returns {string} 格式如 "#RRGGBB" 的字符串。
 */
function rgbToHex(r, g, b) {
    // 将每个分量转换为两位十六进制数，不足则补零
    const toHex = c => ('0' + c.toString(16)).slice(-2);
    return `#${toHex(r)}${toHex(g)}${toHex(b)}`.toUpperCase();
}

/**
 * 将 RGB 颜色分量转换为 CSS 的 rgb() 格式字符串。
 * @param {number} r - 红色分量 (0-255)。
 * @param {number} g - 绿色分量 (0-255)。
 * @param {number} b - 蓝色分量 (0-255)。
 * @returns {string} 格式如 "rgb(r, g, b)" 的字符串。
 */
function rgbToRgbString(r, g, b) {
    return `rgb(${r}, ${g}, ${b})`;
}

// --- 核心功能函数 ---

/**
 * 设置画布，加载并显示全屏背景图。
 * 此函数现在会在每次截图时被调用，以更新背景。
 * @param {string} screenshotDataUrl - 后端传递的包含全屏截图的 Base64 Data URL。
 */
function setupCanvas(screenshotDataUrl) {
    // 设置画布尺寸以匹配窗口大小
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    // 鲁棒性检查：确保接收到的数据是有效的字符串
    if (!screenshotDataUrl || typeof screenshotDataUrl !== 'string') {
        console.error("接收到的截图数据无效。");
        alert("未能加载截图数据，窗口将关闭。");
        appWindow.close();
        return;
    }

    // 创建一个新的 Image 对象来加载截图
    screenCapture = new Image();
    screenCapture.onload = () => {
        console.log("全屏截图加载完成，开始绘制界面。");
        // 图片加载成功后，立即进行首次绘制
        draw();
    };
    screenCapture.onerror = (err) => {
        console.error("加载截图数据URL失败:", err);
        alert("无法加载截图，请重试。");
        appWindow.close();
    };
    // 设置图像源，这将触发加载
    screenCapture.src = screenshotDataUrl;
}

/**
 * 绘制鼠标指针位置的放大镜效果，并显示颜色信息。
 */
function drawMagnifier() {
    // 如果截图未加载或鼠标不在画布内，则不进行绘制
    if (!screenCapture || !currentX) return;

    // --- 放大镜参数 ---
    const magnifierSize = 160;
    const zoomFactor = 2;
    const borderWidth = 3;
    const infoBoxHeight = 40;

    // 放大镜定位在画布右上角
    const magnifierX = canvas.width - magnifierSize - 20;
    const magnifierY = 20;

    ctx.save(); // 保存当前绘图状态，以便后续恢复

    // 1. 创建圆形剪裁区域，使放大镜呈圆形
    ctx.beginPath();
    ctx.arc(magnifierX + magnifierSize / 2, magnifierY + magnifierSize / 2, magnifierSize / 2, 0, Math.PI * 2);
    ctx.clip(); // 应用剪裁，后续的绘制将只在此圆形区域内显示

    // 2. 绘制放大的图像内容
    const sourceX = currentX - (magnifierSize / zoomFactor / 2);
    const sourceY = currentY - (magnifierSize / zoomFactor / 2);
    const sourceWidth = magnifierSize / zoomFactor;
    const sourceHeight = magnifierSize / zoomFactor;
    ctx.drawImage(screenCapture,
        sourceX, sourceY, sourceWidth, sourceHeight,
        magnifierX, magnifierY, magnifierSize, magnifierSize);

    // 3. 绘制颜色信息框
    if (currentColor) {
        ctx.fillStyle = 'rgba(0, 0, 0, 0.7)';
        ctx.fillRect(magnifierX, magnifierY + magnifierSize - infoBoxHeight, magnifierSize, infoBoxHeight);

        ctx.fillStyle = currentColor.hex;
        ctx.fillRect(magnifierX + 10, magnifierY + magnifierSize - infoBoxHeight + 10, 20, 20);

        ctx.fillStyle = '#fff';
        ctx.font = '14px Arial';
        const textToShow = colorFormat === 'hex' ? currentColor.hex : currentColor.rgb;
        ctx.fillText(textToShow, magnifierX + 40, magnifierY + magnifierSize - infoBoxHeight + 25);
    }

    // 4. 绘制边框（根据是否复制成功改变颜色）
    ctx.strokeStyle = copyFeedback ? '#4cd964' : 'rgba(255, 255, 255, 0.8)';
    ctx.lineWidth = borderWidth;
    ctx.stroke();

    // 5. 绘制中心十字准星
    ctx.strokeStyle = '#ff0000';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(magnifierX + magnifierSize / 2 - 10, magnifierY + magnifierSize / 2);
    ctx.lineTo(magnifierX + magnifierSize / 2 + 10, magnifierY + magnifierSize / 2);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(magnifierX + magnifierSize / 2, magnifierY + magnifierSize / 2 - 10);
    ctx.lineTo(magnifierX + magnifierSize / 2, magnifierY + magnifierSize / 2 + 10);
    ctx.stroke();

    ctx.restore(); // 恢复绘图状态（移除剪裁）
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

    // 智能定位，防止指示器超出画布边界
    let textX = rectX + width + 5;
    let textY = rectY + height + 20;

    if (textX + ctx.measureText(text).width > canvas.width) {
        textX = rectX - ctx.measureText(text).width - 5;
    }
    if (textY > canvas.height) {
        textY = rectY - 10;
    }

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
    // 1. 清除整个画布
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // 2. 绘制全屏截图作为背景
    if (screenCapture) {
        ctx.drawImage(screenCapture, 0, 0, canvas.width, canvas.height);
    }

    // 3. 绘制半透明的遮罩层
    ctx.fillStyle = 'rgba(0, 0, 0, 0.5)';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // 4. 如果正在绘制选区，则高亮显示选区内容
    if (isDrawing) {
        const width = currentX - startX;
        const height = currentY - startY;
        const realX = Math.min(startX, currentX);
        const realY = Math.min(startY, currentY);
        const realW = Math.abs(width);
        const realH = Math.abs(height);

        // 从背景图中“抠出”选区部分并绘制，实现高亮效果
        if (screenCapture && realW > 0 && realH > 0) {
            ctx.drawImage(screenCapture, realX, realY, realW, realH, realX, realY, realW, realH);
        }
        // 绘制选区边框
        ctx.strokeStyle = 'rgba(97, 175, 239, 0.9)';
        ctx.lineWidth = 2;
        ctx.strokeRect(startX, startY, width, height);
    }

    // 5. 绘制辅助工具
    drawMagnifier();
    drawSizeIndicator();
}

/**
 * 封装取消截图的逻辑：隐藏窗口并通知后端释放截图锁。
 */
async function cancel_screenshot() {
    console.log("截图已取消，正在隐藏窗口并通知后端。");
    await appWindow.hide();
    try {
        await invoke('cancel_screenshot');
    } catch (error) {
        console.error("调用后端 'cancel_screenshot' 失败:", error);
    }
}

/**
 * 复制当前颜色到剪贴板，并提供视觉反馈。
 */
async function copyColorToClipboard() {
    if (!currentColor) return;

    const textToCopy = colorFormat === 'hex' ? currentColor.hex : currentColor.rgb;
    try {
        await writeText(textToCopy);
        console.log(`颜色值已复制: ${textToCopy}`);
        // 触发视觉反馈
        copyFeedback = true;
        setTimeout(() => {
            copyFeedback = false; // 300毫秒后恢复正常
        }, 300);
    } catch (error) {
        console.error("复制颜色失败:", error);
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
    requestAnimationFrame(draw);
});

canvas.addEventListener('mousemove', (e) => {
    // 更新当前鼠标坐标
    currentX = e.clientX;
    currentY = e.clientY;

    // 实时获取鼠标下方像素颜色
    if (screenCapture) {
        const pixelData = ctx.getImageData(currentX, currentY, 1, 1).data;
        const [r, g, b] = pixelData;
        currentColor = {
            r, g, b,
            hex: rgbToHex(r, g, b),
            rgb: rgbToRgbString(r, g, b),
        };
    }

    // 请求浏览器在下一帧重绘画布
    requestAnimationFrame(draw);
});

canvas.addEventListener('mouseup', async (e) => {
    if (e.button !== 0 || !isDrawing) return;
    isDrawing = false;
    await appWindow.hide();

    const x = Math.min(startX, currentX);
    const y = Math.min(startY, currentY);
    const width = Math.abs(currentX - startX);
    const height = Math.abs(currentY - startY);

    // 忽略过小的无效选区
    if (width < 10 || height < 10) {
        await cancel_screenshot();
        return;
    }

    // 将有效的选区信息发送给后端进行处理
    try {
        await invoke('process_screenshot_area', { x, y, width, height });
    } catch (error) {
        console.error("调用后端 'process_screenshot_area' 指令失败:", error);
        await cancel_screenshot(); // 即使失败也要确保取消截图状态
    }
});

// 监听鼠标右键点击，用于取消截图
canvas.addEventListener('contextmenu', async (e) => {
    e.preventDefault(); // 阻止默认的右键菜单
    await cancel_screenshot();
});

// 监听键盘事件，提供快捷操作
document.addEventListener('keydown', async (e) => {
    e.preventDefault(); // 阻止浏览器默认行为

    switch (e.key.toLowerCase()) {
        case 'escape':
            await cancel_screenshot();
            break;
        case 'shift':
            // 切换颜色格式 (HEX <-> RGB)
            colorFormat = (colorFormat === 'hex') ? 'rgb' : 'hex';
            requestAnimationFrame(draw); // 立即重绘以更新显示
            break;
        case 'c':
            // 复制颜色值
            await copyColorToClipboard();
            requestAnimationFrame(draw); // 立即重绘以显示视觉反馈
            break;
    }
});

// --- 初始化逻辑 ---

/**
 * 初始化截图窗口，并设置一个持久的事件监听器。
 * @async
 */
async function initialize() {
    console.log("截图窗口前端已加载，等待后端推送初始化数据...");

    // --- 核心修复 ---
    // 问题：原先的监听器在执行一次后会通过 unlisten() 自我注销，
    //       导致第二次截图时，窗口虽然被重用，但无法接收新的截图数据，
    //       从而显示陈旧的（第一次的）截图内容。
    // 方案：移除 unlisten() 调用，将监听器变为持久性的。
    //       这样，每次后端重用此窗口并发送 'initialize-screenshot' 事件时，
    //       都能被正确捕获，并调用 setupCanvas 更新背景。
    await listen('initialize-screenshot', (event) => {
        console.log("接收到来自后端的初始化事件:", event);

        // 鲁棒性检查：确保收到的数据是有效的
        if (event.payload && event.payload.image_data_url) {
            setupCanvas(event.payload.image_data_url);
        } else {
            console.error("初始化事件的载荷无效:", event.payload);
            alert("初始化截图失败：数据错误。");
            appWindow.close();
        }
    });
}

// 启动初始化流程
initialize();