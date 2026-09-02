import os
import io
import struct
import cairosvg
from PIL import Image
import subprocess
import platform
import sys

def ensure_directory(directory):
    """确保目录存在，如果不存在则创建"""
    if not os.path.exists(directory):
        os.makedirs(directory)

def svg_to_png(svg_path, output_path, width, height):
    """将 SVG 转换为指定尺寸的 PNG（超采样渲染 + 透明边自动裁剪后精确缩放到目标尺寸）"""
    # 以 4 倍目标尺寸渲染 SVG，保证小尺寸下圆环边缘的抗锯齿质量
    png_data = cairosvg.svg2png(
        url=svg_path,
        output_width=width * 4,
        output_height=height * 4,
    )
    img = Image.open(io.BytesIO(png_data)).convert("RGBA")

    # 裁剪四周透明区域，让图标内容占满画布（消除 SVG 视口留白导致的图标视觉缩小）
    alpha_bbox = img.split()[3].getbbox()
    if alpha_bbox:
        img = img.crop(alpha_bbox)

    # 缩放到目标尺寸
    img = img.resize((width, height), Image.LANCZOS)

    # 使用 PIL 设置 DPI 为 72
    img.save(output_path, dpi=(72, 72))
    img.close()
    print(f"已创建: {output_path} (72 DPI)")

def create_retina_image(png_path, output_path, scale=2):
    """创建 Retina (@2x) 版本的图像"""
    img = Image.open(png_path)
    width, height = img.size
    retina_img = img.resize((width * scale, height * scale), Image.LANCZOS)
    retina_img.save(output_path, dpi=(72, 72))
    print(f"已创建: {output_path} (72 DPI)")

def create_ico(png_path, output_path):
    """创建 .ico 文件（内嵌 16~256 多尺寸，256 由 PIL 以 PNG 压缩写入）"""
    # Pillow 写入 ICO 时按尺寸升序排列帧，但 Tauri 编译期只取 ICO 第一帧
    # （tauri-codegen 的 CachedIcon::new_ico 读 entries()[0]）作为窗口/任务栏图标，
    # 首帧若为 16px，任务栏放大后图标会模糊。因此保存后重排目录，使 256px 帧居首。
    sizes = [(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (24, 24), (16, 16)]

    img = Image.open(png_path)
    try:
        # 直接以源图为基准按 sizes 逐尺寸缩放，Pillow 内部生成多帧 ICO。
        # 不要传入 append_images：Pillow 12 的 ICO 插件只写入不大于主图的尺寸，
        # 小图为主图时其余尺寸会被跳过（且其句柄不释放，导致后续删除临时文件报 WinError 32）。
        img.save(output_path, format='ICO', sizes=sizes)
    finally:
        img.close()

    reorder_ico_largest_first(output_path)

    print(f"已创建: {output_path}")

def reorder_ico_largest_first(ico_path):
    """重排 ICO 目录项，使最大尺寸帧排在首位（Pillow 写入时为升序）。"""
    with open(ico_path, 'rb') as f:
        data = f.read()
    count = struct.unpack('<H', data[4:6])[0]

    entries = []
    for i in range(count):
        offset = 6 + 16 * i
        width = data[offset] or 256
        height = data[offset + 1] or 256
        size = struct.unpack('<I', data[offset + 8:offset + 12])[0]
        data_offset = struct.unpack('<I', data[offset + 12:offset + 16])[0]
        entries.append((width, height, data[offset:offset + 16], data[data_offset:data_offset + size]))

    entries.sort(key=lambda e: -e[0])  # 按宽度降序，最大帧居首

    header = data[:6]
    directory = b''.join(e[2] for e in entries)
    payload = b''.join(e[3] for e in entries)

    out = bytearray(header + directory)
    cursor = 6 + 16 * count
    for i, (_, _, _, blob) in enumerate(entries):
        offset = 6 + 16 * i
        out[offset + 8:offset + 12] = struct.pack('<I', len(blob))
        out[offset + 12:offset + 16] = struct.pack('<I', cursor)
        cursor += len(blob)
    out += payload

    with open(ico_path, 'wb') as f:
        f.write(out)

def create_icns(svg_path, output_path):
    """创建 .icns 文件 (仅在 macOS 上使用 iconutil)"""
    # 创建临时目录
    temp_iconset = "icon.iconset"
    ensure_directory(temp_iconset)

    # 创建不同尺寸的图标
    sizes = [16, 32, 64, 128, 256, 512, 1024]
    for size in sizes:
        # 标准尺寸
        standard_output = os.path.join(temp_iconset, f"icon_{size}x{size}.png")
        svg_to_png(svg_path, standard_output, size, size)

        # Retina 尺寸
        if size < 512:  # 1024 不需要 @2x 版本
            retina_output = os.path.join(temp_iconset, f"icon_{size}x{size}@2x.png")
            svg_to_png(svg_path, retina_output, size * 2, size * 2)

    # 使用 iconutil 创建 .icns 文件 (仅在 macOS 上)
    if platform.system() == 'Darwin':
        subprocess.run(['iconutil', '-c', 'icns', temp_iconset, '-o', output_path])
        print(f"已创建: {output_path}")
    else:
        print("警告: 创建 .icns 文件需要 macOS 系统和 iconutil 工具")

    # 清理临时目录
    for file in os.listdir(temp_iconset):
        os.remove(os.path.join(temp_iconset, file))
    os.rmdir(temp_iconset)

def main():
    # 输入 SVG 文件
    svg_file = "icon.svg"

    # 检查文件是否存在
    if not os.path.exists(svg_file):
        print(f"错误: 找不到文件 {svg_file}")
        sys.exit(1)

    # 创建输出目录
    output_dir = "output"
    ensure_directory(output_dir)

    # 创建标准 PNG 图标
    standard_png = os.path.join(output_dir, "icon.png")
    svg_to_png(svg_file, standard_png, 1024, 1024)  # 创建一个高分辨率的基础 PNG

    # 创建指定尺寸的 PNG 图标
    sizes = {
        "32x32.png": (32, 32),
        "128x128.png": (128, 128),
        "Square30x30Logo.png": (30, 30),
        "Square44x44Logo.png": (44, 44),
        "Square71x71Logo.png": (71, 71),
        "Square89x89Logo.png": (89, 89),
        "Square107x107Logo.png": (107, 107),
        "Square142x142Logo.png": (142, 142),
        "Square150x150Logo.png": (150, 150),
        "Square284x284Logo.png": (284, 284),
        "Square310x310Logo.png": (310, 310),
        "StoreLogo.png": (50, 50)  # 假设 StoreLogo 为 50x50
    }

    for filename, (width, height) in sizes.items():
        output_path = os.path.join(output_dir, filename)
        svg_to_png(svg_file, output_path, width, height)

    # 创建 Retina (@2x) 版本
    retina_path = os.path.join(output_dir, "128x128@2x.png")
    svg_to_png(svg_file, retina_path, 256, 256)  # 直接从 SVG 创建 2x 尺寸

    # 创建 .ico 文件
    ico_path = os.path.join(output_dir, "icon.ico")
    create_ico(standard_png, ico_path)

    # 创建 .icns 文件 (仅在 macOS 上)
    icns_path = os.path.join(output_dir, "icon.icns")
    create_icns(svg_file, icns_path)

    print("所有图标已成功创建，分辨率均为 72 DPI！")

if __name__ == "__main__":
    main()
