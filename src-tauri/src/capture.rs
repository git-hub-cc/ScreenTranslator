use xcap::Monitor;
use image::RgbaImage;
use std::io::Cursor;
use base64::{Engine as _, engine::general_purpose};
// --- 新增：引入 png 库的相关模块以进行性能优化 ---
use png::Compression;

/// 捕获主显示器的全屏图像。
///
/// # 返回
///
/// `Result<RgbaImage, String>`:
/// - `Ok(RgbaImage)`: 成功捕获到的 RGBA 格式的图像缓冲区。
/// - `Err(String)`: 捕获过程中发生的错误信息。
pub fn capture_fullscreen() -> Result<RgbaImage, String> {
    // 1. 获取所有连接的显示器
    let monitors = Monitor::all().map_err(|e| format!("无法获取显示器列表: {}", e))?;
    if monitors.is_empty() {
        return Err("未找到任何显示器".to_string());
    }

    // 2. 查找主显示器
    let primary_monitor = monitors.into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| Monitor::all().ok()?.into_iter().next()) // 如果没有主显示器，就用第一个
        .ok_or_else(|| "无法确定要捕获的显示器".to_string())?;

    let monitor_name = primary_monitor.name().unwrap_or_else(|_| "未知名称".to_string());
    let monitor_width = primary_monitor.width().unwrap_or(0);
    let monitor_height = primary_monitor.height().unwrap_or(0);

    println!(
        "准备在主显示器上截图: (名称={}, 尺寸={}x{})",
        monitor_name,
        monitor_width,
        monitor_height
    );

    // 3. 执行截图操作
    let image = primary_monitor
        .capture_image()
        .map_err(|e| format!("在显示器 '{}' 上截图失败: {}", monitor_name, e))?;

    println!("全屏截图成功，图像尺寸: {}x{}", image.width(), image.height());

    // 4. 返回图像
    Ok(image)
}


/// 将图像缓冲区编码为 Base64 格式的 Data URL。
///
/// # 参数
/// - `image`: 要编码的图像缓冲区 (`RgbaImage`)。
///
/// # 返回
///
/// `Result<String, String>`:
/// - `Ok(String)`: 格式为 "data:image/png;base64,..." 的字符串。
/// - `Err(String)`: 编码过程中发生的错误。
pub fn encode_image_to_data_url(image: &RgbaImage) -> Result<String, String> {
    let mut buffer = Cursor::new(Vec::new());

    // --- 核心性能优化：使用 png 库并设置快速压缩 ---
    // 1. 创建一个 PNG编码器
    let mut encoder = png::Encoder::new(&mut buffer, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    // 2. 设置压缩级别为 `Fast`，这是显著提升编码速度的关键
    encoder.set_compression(Compression::Fast);

    // 3. 获取编码器的写入器并写入图像数据
    let mut writer = encoder.write_header()
        .map_err(|e| format!("写入PNG头失败: {}", e))?;
    writer.write_image_data(image.as_raw())
        .map_err(|e| format!("写入PNG图像数据失败: {}", e))?;
    writer.finish().map_err(|e| format!("完成PNG编码失败: {}", e))?;
    // --- 优化结束 ---

    // 4. 对内存中的 PNG 数据进行 Base64 编码
    let base64_str = general_purpose::STANDARD.encode(buffer.get_ref());

    // 5. 构造成前端可以直接使用的 Data URL 格式
    Ok(format!("data:image/png;base64,{}", base64_str))
}