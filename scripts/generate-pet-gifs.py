"""Make transparent GIFs from the ACTUAL pet image, without redrawing it.

    python scripts/generate-pet-gifs.py
    python scripts/generate-pet-gifs.py --still-only

Requires Pillow, numpy and scipy. The source JPG and application settings are
never modified. Generated files go to artifacts/pet-bear-original, NOT the old
pet-bear cartoon directory. GIF is limited to 255 opaque colors and binary alpha;
bear.png keeps the source resolution and original opaque-interior RGB pixels.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path

import numpy as np
from PIL import GifImagePlugin, Image, ImageDraw
from scipy import ndimage

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src-tauri" / "assets" / "pet-default.jpg"
OUTPUT = ROOT / "artifacts" / "pet-bear-original"
FRAMES = 20
FRAME_MS = 100
TRANSPARENT = 255
MAX_BYTES = 5 * 1024 * 1024
ACTIONS = ("idle", "walk", "greet", "working", "celebrate", "sad", "drag", "teleport")


def remove_white_background(source: Image.Image) -> tuple[Image.Image, dict]:
    """Flood only edge-connected near-white pixels; protect enclosed eye whites.

    A narrow 3-pixel boundary band estimates alpha against the white matte using
    the nearest opaque color. Only this band is decontaminated; interior pixels
    are copied exactly. Do not globally replace whites or recolor the character.
    """
    rgb = np.asarray(source.convert("RGB"))
    low = rgb.min(axis=2).astype(np.int16)
    high = rgb.max(axis=2).astype(np.int16)
    near_white = (low >= 235) & (high - low <= 20)
    seeds = np.zeros(near_white.shape, dtype=bool)
    seeds[0, :] = near_white[0, :]
    seeds[-1, :] = near_white[-1, :]
    seeds[:, 0] = near_white[:, 0]
    seeds[:, -1] = near_white[:, -1]
    background = ndimage.binary_propagation(seeds, mask=near_white)
    foreground = ~background
    solid = ndimage.binary_erosion(foreground, iterations=2)
    if not solid.any():
        raise ValueError("No opaque subject found in source image")
    candidate = ndimage.binary_dilation(foreground, iterations=1)
    edge = candidate & ~solid
    nearest = ndimage.distance_transform_edt(~solid, return_distances=False,
                                             return_indices=True)
    observed = rgb.astype(np.float64)
    reference = observed[nearest[0], nearest[1]]
    white_minus_reference = 255.0 - reference
    denominator = (white_minus_reference ** 2).sum(axis=2)
    estimated_alpha = np.clip(
        ((255.0 - observed) * white_minus_reference).sum(axis=2)
        / np.maximum(denominator, 1.0), 0.0, 1.0,
    )
    alpha = np.zeros(near_white.shape, dtype=np.float64)
    alpha[solid] = 1.0
    alpha[edge] = estimated_alpha[edge]
    # Suppress JPEG noise in the white background, rather than keeping a halo.
    alpha[alpha < 0.06] = 0.0
    restored = observed.copy()
    restore_edge = edge & (alpha > 0)
    restored[restore_edge] = np.clip(
        (observed[restore_edge] - 255.0 * (1.0 - alpha[restore_edge, None]))
        / alpha[restore_edge, None], 0.0, 255.0,
    )
    result = np.dstack((np.rint(restored).astype(np.uint8),
                        np.rint(alpha * 255).astype(np.uint8)))
    result[alpha == 0, :3] = 0
    assert np.array_equal(result[solid, :3], rgb[solid])
    assert np.all(result[solid, 3] == 255)
    # White pixels enclosed by the bear must never be mistaken for backdrop.
    enclosed_white = near_white & ~background
    assert np.all(result[enclosed_white, 3] == 255)
    image = Image.fromarray(result)
    return image, {
        "sourceSize": list(source.size),
        "sourceInteriorPixelsPreserved": int(solid.sum()),
        "enclosedWhitePixelsPreserved": int(enclosed_white.sum()),
        "subjectBounds": list(image.getbbox()),
    }


def composite(image: Image.Image, color: str) -> Image.Image:
    surface = Image.new("RGBA", image.size, color)
    surface.alpha_composite(image)
    return surface.convert("RGB")


def cutout_preview(source: Image.Image, cutout: Image.Image, output: Path) -> None:
    w, h = source.size
    preview = Image.new("RGB", (w * 3, h + 30), "white")
    panels = [source.convert("RGB"), composite(cutout, "#ffffff"),
              composite(cutout, "#242b35")]
    draw = ImageDraw.Draw(preview)
    for i, (panel, label) in enumerate(zip(panels, ["Original JPG", "Transparent PNG / white", "Transparent PNG / dark"])):
        preview.paste(panel, (w * i, 0))
        draw.text((w * i + 12, h + 8), label, fill="#333333")
    preview.save(output / "source-comparison.png")


def render(cutout: Image.Image, action: str, index: int) -> Image.Image:
    """Only rigid transforms: no redraw, new facial expression or limb poses."""
    phase = math.tau * index / FRAMES
    dx = dy = angle = 0.0
    if action == "idle":
        dy = -1.5 * (1.0 - math.cos(phase))
    elif action == "walk":
        dy = -3.0 * (1.0 - math.cos(2.0 * phase))
        angle = 1.6 * math.sin(phase)
    elif action == "greet":
        angle = 3.0 * math.sin(phase)
    elif action == "working":
        dy = -2.0 * (1.0 - math.cos(2.0 * phase))
    elif action == "celebrate":
        dy = -10.0 * (1.0 - math.cos(2.0 * phase))
        angle = 2.0 * math.sin(phase)
    elif action == "sad":
        dx = 3.0 * math.sin(2.0 * phase)
    elif action == "drag":
        angle = 4.0 * math.sin(phase)
        dx = 3.0 * math.sin(phase)
    elif action == "teleport":
        # No shape-changing squash: briefly disappear, then reappear unchanged.
        if index in (9, 10):
            return Image.new("RGBA", cutout.size)
    else:
        raise ValueError(action)

    bounds = cutout.getbbox()
    anchor = ((bounds[0] + bounds[2]) / 2.0, bounds[3])
    rotated = cutout if abs(angle) < 1e-8 else cutout.rotate(
        angle, Image.Resampling.BICUBIC, center=anchor,
    )
    result = Image.new("RGBA", cutout.size)
    result.alpha_composite(rotated, (round(dx), round(dy)))
    bounds = result.getbbox()
    if bounds:
        # Source has ample whitespace. Never shrink the bear to fit animations.
        assert min(bounds[0], bounds[1], result.width - bounds[2], result.height - bounds[3]) > 8
    return result


def make_palette(cutout: Image.Image) -> Image.Image:
    rgba = np.asarray(cutout)
    visible_rgb = rgba[rgba[:, :, 3] >= 128, :3]
    # Sample only the subject, not its white/transparent background.
    samples = Image.fromarray(visible_rgb.reshape(-1, 1, 3))
    palette = samples.quantize(colors=255, method=Image.Quantize.MEDIANCUT)
    colors = palette.getpalette()[:255 * 3]
    colors += [0] * (255 * 3 - len(colors))
    # Duplicate entry 0 at reserved transparent slot; remap any ties to 0 below.
    palette.putpalette(colors + colors[:3])
    return palette


def indexed(image: Image.Image, palette: Image.Image) -> Image.Image:
    result = image.convert("RGB").quantize(palette=palette, dither=Image.Dither.NONE)
    data = np.array(result)
    data[data == TRANSPARENT] = 0
    data[np.asarray(image)[:, :, 3] < 128] = TRANSPARENT
    result = Image.fromarray(data).convert("P")
    result.putpalette(palette.getpalette())
    result.info["transparency"] = TRANSPARENT
    return result


def same_visible_pixels(actual: Image.Image, expected: Image.Image) -> bool:
    actual_rgba = np.asarray(actual.convert("RGBA"))
    expected_rgba = np.asarray(expected.convert("RGBA"))
    if not np.array_equal(actual_rgba[:, :, 3], expected_rgba[:, :, 3]):
        return False
    visible = expected_rgba[:, :, 3] > 0
    return np.array_equal(actual_rgba[visible, :3], expected_rgba[visible, :3])


def validate_gif(path: Path, expected: list[Image.Image]) -> dict:
    """Verify full decoded frames, including alpha: catch disposal/trail errors."""
    elapsed = 0
    encoded_frames = 0
    with Image.open(path) as gif:
        assert gif.size == expected[0].size
        assert gif.info["loop"] == 0
        encoded_frames = gif.n_frames
        for i in range(encoded_frames):
            gif.seek(i)
            decoded = gif.convert("RGBA")
            duration = gif.info["duration"]
            assert duration > 0 and duration % FRAME_MS == 0
            # Identical consecutive frames are merged. Check every time slot.
            for timestamp in range(elapsed, elapsed + duration, FRAME_MS):
                assert timestamp // FRAME_MS < len(expected)
                assert same_visible_pixels(decoded, expected[timestamp // FRAME_MS]), (path.name, i, timestamp)
            assert decoded.getpixel((0, 0))[3] == 0
            assert decoded.getpixel((decoded.width - 1, decoded.height - 1))[3] == 0
            elapsed += duration
    assert elapsed == len(expected) * FRAME_MS
    size = path.stat().st_size
    assert size <= MAX_BYTES, (path.name, size, "exceeds the app's 5 MiB limit")
    return {"file": path.name, "size": list(expected[0].size), "frames": encoded_frames,
            "durationMs": elapsed, "bytes": size, "decodedFramesMatch": True}


def save_gif(path: Path, frames: list[Image.Image]) -> dict:
    # Write complete frame blocks: Pillow's high-level bounding-box optimizer
    # can drop all-transparent frames even with optimize=False. Such a frame
    # is meaningful for teleport, so do not let a crop optimizer remove it.
    # Pillow may promote a P image's palette to RGBA when loading/converting it.
    # Capture explicit RGB palette bytes and serialize fresh images, so validation
    # or an earlier standalone export cannot corrupt the combined GIF's header.
    colors = bytes(frames[0].getpalette("RGB"))
    assert len(colors) == 768
    runs = []
    previous_pixels = None
    for image in frames:
        assert bytes(image.getpalette("RGB")) == colors
        pixels = image.tobytes()
        if pixels == previous_pixels:
            runs[-1][1] += FRAME_MS
        else:
            runs.append([pixels, FRAME_MS])
        previous_pixels = pixels

    def encoder_image(pixels: bytes) -> Image.Image:
        image = Image.frombytes("P", frames[0].size, pixels)
        image.putpalette(colors, rawmode="RGB")
        return image

    with path.open("wb") as stream:
        header, _ = GifImagePlugin.getheader(encoder_image(runs[0][0]), info={
            "loop": 0, "background": TRANSPARENT, "transparency": TRANSPARENT,
        })
        stream.writelines(header)
        for pixels, duration in runs:
            stream.writelines(GifImagePlugin.getdata(
                encoder_image(pixels), duration=duration, disposal=2,
                transparency=TRANSPARENT,
            ))
        stream.write(b";")
    report = validate_gif(path, frames)
    print(f"{path.name}: {report['frames']} encoded frames, {report['durationMs']} ms, {report['bytes']:,} bytes; decoded frames verified")
    return report


def decoded_frame(path: Path, timestamp: int) -> Image.Image:
    with Image.open(path) as gif:
        elapsed = 0
        for i in range(gif.n_frames):
            gif.seek(i)
            elapsed += gif.info["duration"]
            if timestamp < elapsed:
                return gif.convert("RGBA")
    raise ValueError(timestamp)


def previews(output: Path) -> None:
    # Preview the actual decoded GIF (including quantization), not source frames.
    size = 300
    sheet = Image.new("RGB", (size * 4, (size + 24) * len(ACTIONS)), "white")
    draw = ImageDraw.Draw(sheet)
    for row, action in enumerate(ACTIONS):
        for column, timestamp in enumerate((0, 500, 1000, 1500)):
            color = "#242b35" if column % 2 == 0 else "#f4f0e9"
            image = decoded_frame(output / f"{action}.gif", timestamp)
            panel = composite(image, color).resize((size, size), Image.Resampling.LANCZOS)
            x, y = column * size, row * (size + 24)
            sheet.paste(panel, (x, y))
            draw.text((x + 8, y + size + 5), f"{action} / {timestamp} ms", fill="#333333")
    sheet.save(output / "preview.png")
    cards = "".join(
        f'<figure><img src="{action}.gif" alt="{action}"><figcaption>{action}</figcaption></figure>'
        for action in ACTIONS
    )
    html = '''<!doctype html><html lang="zh-CN"><meta charset="utf-8">
<title>原图桌宠 GIF 预览</title><style>
body{font:16px system-ui;background:#242b35;color:#eee;margin:24px}button{padding:8px 16px}
main{display:flex;flex-wrap:wrap}figure{margin:12px}img{width:250px;height:250px;object-fit:contain}
.light{background:#f4f0e9;color:#222}.checker{background-color:#eee;color:#222;
background-image:conic-gradient(#ccc 25%,transparent 0 50%,#ccc 0 75%,transparent 0);background-size:24px 24px}
</style><h1>原图透明桌宠</h1><p>使用原 JPG，未重绘五官和身体。以下是原姿势的整体动效，不是新的肢体动作。</p>
<button onclick="document.body.className=''">深色背景</button>
<button onclick="document.body.className='light'">浅色背景</button>
<button onclick="document.body.className='checker'">透明网格</button>
<main>''' + cards + '''</main><h2>所有动效依次播放</h2>
<img src="all-actions.gif" alt="所有动效"><p>当前应用不会按鼠标事件切换动作；这里仅预览素材。</p></html>'''
    (output / "preview.html").write_text(html, encoding="utf-8")


def write_readme(output: Path) -> None:
    text = """# 原图小熊：透明背景素材（非重绘版）

这次直接使用 `src-tauri/assets/pet-default.jpg` 的原始像素，保留原来的脸、体型、毛发、颜色和姿势；没有重新绘制卡通熊。旧的 `artifacts/pet-bear` 是已弃用的卡通版本，**请改用本目录**。

## 文件与效果

- `bear.png`：500×500 透明 PNG。保留原图不透明内部像素；仅在轮廓附近去白底、去白边。这是保留细节最好的版本。
- `idle.gif`：原姿势轻微浮动，推荐日常使用。
- `walk.gif`：原图小幅颠动和摇摆；不是重新绘制的迈腿走路。
- `greet.gif`：悬停问候用的轻摆；不是挥手动作。
- `working.gif`：稍快的浮动，脸和手脚保持原样。
- `celebrate.gif`：原图轻跳。
- `sad.gif`：任务失败时轻微横向摇动，**没有改成哭脸或其他表情**。
- `drag.gif`：拖动时整体轻摆。
- `teleport.gif`：短暂隐藏后原样出现，不拉伸形象。
- `all-actions.gif`：上述效果按顺序循环。
- `source-comparison.png`：原 JPG 与透明 PNG 在白色/深色背景上的对照。
- `preview.png`：从实际 GIF 解码生成的动作联系表。
- `preview.html`：在浏览器中播放所有 GIF，可切换深色、浅色与透明网格背景。
- `validation.json`：源图片 SHA-256、抠图保真检查与各 GIF 解码验证结果。

所有 GIF 都是原图的 500×500 画布，无限循环。每个动作 2 秒，汇总 16 秒。GIF 只能保留最多 255 个不透明颜色及二值透明，因此毛发颜色和边缘不可能与原 JPG 逐像素完全相同；PNG 保留更完整的细节和半透明边缘。没有凭空生成不同的手脚姿势，也没有对脸部重新绘制。

## 如何使用

设置 → 通知与桌宠 → 桌宠形象 → 选择图片，**重新选择本目录中的 `idle.gif`、`all-actions.gif` 或 `bear.png`**。已经导入过旧图时必须重新选择，因为应用会复制素材，不会自动刷新已导入的文件。

当前应用只导入单张图片/动图。汇总 GIF 只能依次循环，不能按鼠标操作选择其中某个动作；独立 GIF 的状态切换功能尚未接入。这次没有替换默认资源、修改设置或重启应用。

## 重新生成

依赖：Python、Pillow、numpy、scipy。在仓库根目录运行：

```
python scripts/generate-pet-gifs.py
```

只生成透明 PNG 及对照图：

```
python scripts/generate-pet-gifs.py --still-only
```

本目录是生成产物；脚本包含 README 和预览页的生成逻辑，保留原 JPG 和脚本即可重建。验证会逐帧对比 GIF 解码后的完整 alpha 及可见像素，确认透明清屏无残影，并检查时长、循环、裁切和 5 MiB 大小上限。
"""
    (output / "README.md").write_text(text, encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--still-only", action="store_true", help="Only generate the transparent PNG and source comparison")
    args = parser.parse_args()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with Image.open(SOURCE) as original:
        source = original.convert("RGB")
    cutout, report = remove_white_background(source)
    cutout.save(OUTPUT / "bear.png")
    cutout_preview(source, cutout, OUTPUT)
    write_readme(OUTPUT)
    report["source"] = SOURCE.relative_to(ROOT).as_posix()
    report["sha256"] = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    report["gifResults"] = []
    if not args.still_only:
        palette = make_palette(cutout)
        combined = []
        for action in ACTIONS:
            frames = [indexed(render(cutout, action, i), palette) for i in range(FRAMES)]
            report["gifResults"].append(save_gif(OUTPUT / f"{action}.gif", frames))
            combined.extend(frames)
        report["gifResults"].append(save_gif(OUTPUT / "all-actions.gif", combined))
        previews(OUTPUT)
    (OUTPUT / "validation.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Output: {OUTPUT}")


if __name__ == "__main__":
    main()
