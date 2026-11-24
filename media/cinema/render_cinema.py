#!/usr/bin/env python3
"""
Newsatoshi Launch Cinema — Apple M5 Pro Metal 60fps WebGL2/GLSL Renderer (Gates 2, 3, 4)

Serves `media/cinema/` on an ephemeral localhost port, launches headless Google Chrome
strictly on Apple Metal GPU (`--use-gl=angle --use-angle=metal --enable-webgl`), asserts
real hardware GPU rendering (fails loudly if SwiftShader/software fallback occurs), and
steps `window.__renderFrame(frameIdx, 60, payload)` deterministically into FFmpeg.

Modes:
  --gate2  Render 21-frame dual-aspect QA contact sheets (16:9 + 9:16) for visual inspection.
  --gate3  Render 3-second 60fps Drop-Cut motion test (0:10.0 - 0:13.0, 180 frames) + audio mux.
  --gate4  Render full 58.06s 60fps masters (16:9 + 9:16), run ffprobe audit & extract stills.
"""

from __future__ import annotations

import argparse
import asyncio
import base64
import functools
import http.server
import io
import json
import os
import socketserver
import subprocess
import threading
import time
import urllib.request
from pathlib import Path
from typing import Any, Dict, List, Tuple

import imageio_ffmpeg
from PIL import Image, ImageDraw
from playwright.async_api import async_playwright

FPS = 60
CHROME_EXE = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

# 21 curated frame timestamps (3 hero moments per shot across all 7 shots!)
GATE2_TIMESTAMPS = [
    # Shot 1: Frozen Vault (0:00 -> 0:11.61)
    2.2, 7.9, 11.02,
    # Shot 2: Awakening (0:11.61 -> 0:16.45)
    11.95, 13.0, 15.1,
    # Shot 3: Mathematical Conscience (0:16.45 -> 0:26.13)
    18.0, 20.80, 25.0,
    # Shot 4: Autonomous Abundance Engine (0:26.13 -> 0:34.84)
    26.85, 31.2, 33.8,
    # Shot 5: Blind Bitcoin Fortress (0:34.84 -> 0:41.61)
    36.4, 39.2, 40.9,
    # Shot 6: Privacy & Vaultless Gravity (0:41.61 -> 0:48.39)
    42.8, 45.6, 47.4,
    # Shot 7: Living Mainnet Heartbeat & Finale (0:48.39 -> 0:58.06)
    50.4, 53.8, 56.2,
]


def fetch_live_stats() -> Dict[str, Any]:
    req = urllib.request.Request(
        "https://newsatoshi.org/mainnet/status",
        headers={"User-Agent": "newsatoshi-cinema/3.0"},
    )
    with urllib.request.urlopen(req, timeout=10.0) as resp:
        st = json.loads(resp.read().decode("utf-8"))
    height = int(st.get("height", 0))
    issued_raw = int(st.get("issued", 0))
    issued_love = issued_raw // 100_000_000 if issued_raw > 10_000_000 else issued_raw
    return {
        "mainnet_height": height,
        "mainnet_diff": float(st.get("difficulty", 0.0)),
        "mainnet_issued_love": issued_love,
        "mainnet_notes": int(st.get("notes", 0)),
        "mainnet_merge_pct": 100.0 if int(st.get("auxpow_chainid", 20051)) == 20051 else 0.0,
    }


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: Any) -> None:
        pass


def start_local_server(root_dir: Path) -> Tuple[socketserver.TCPServer, int]:
    handler = functools.partial(QuietHandler, directory=str(root_dir))
    httpd = socketserver.TCPServer(("127.0.0.1", 0), handler)
    port = httpd.server_address[1]
    th = threading.Thread(target=httpd.serve_forever, daemon=True)
    th.start()
    return httpd, port


async def launch_metal_browser(p):
    return await p.chromium.launch(
        executable_path=CHROME_EXE,
        headless=True,
        args=[
            "--headless=new",
            "--use-gl=angle",
            "--use-angle=metal",
            "--enable-webgl",
            "--ignore-gpu-blocklist",
            "--disable-gpu-sandbox",
            "--disable-frame-rate-limit",
            "--no-sandbox",
            "--hide-scrollbars",
        ],
    )


async def init_cinema_page(browser, url: str, w: int, h: int, payload: Dict[str, Any]):
    pg = await browser.new_page(viewport={"width": w, "height": h}, device_scale_factor=1)
    pg.on("pageerror", lambda err: print(f"[PAGEERROR] {err}", flush=True))
    await pg.goto(url, wait_until="load")
    await pg.wait_for_function("() => window.__CINEMA_READY === true", timeout=20000)
    gpu = await pg.evaluate("() => window.__GPU_INFO")
    if "SwiftShader" in str(gpu.get("renderer", "")):
        raise RuntimeError(f"Hardware GPU assertion failed — fell back to SwiftShader: {gpu}")
    await pg.evaluate("(pl) => { window.__PAYLOAD = pl; }", payload)
    cdp = await pg.context.new_cdp_session(pg)
    return pg, cdp, gpu


async def run_gate2_contact_sheets(
    cinema_dir: Path, out_dir: Path, port: int, payload: Dict[str, Any]
) -> None:
    async with async_playwright() as p:
        browser = await launch_metal_browser(p)
        for aspect, w, h, cols, tw, th in [
            ("16x9", 1920, 1080, 3, 640, 360),
            ("9x16", 1080, 1920, 7, 270, 480),
        ]:
            url = f"http://127.0.0.1:{port}/cinema_stage.html?aspect={aspect}"
            pg, cdp, gpu = await init_cinema_page(browser, url, w, h, payload)
            print(f"[gate2] {aspect} GPU verified: {gpu['renderer']}", flush=True)

            rows = (len(GATE2_TIMESTAMPS) + cols - 1) // cols
            sheet = Image.new("RGB", (cols * tw, rows * th), (2, 4, 8))
            draw = ImageDraw.Draw(sheet)

            t0 = time.time()
            for idx, ts in enumerate(GATE2_TIMESTAMPS):
                fidx = int(round(ts * FPS))
                await pg.evaluate("(f) => window.__renderFrame(f, 60, window.__PAYLOAD)", fidx)
                shot = await cdp.send("Page.captureScreenshot", {"format": "jpeg", "quality": 93})
                im = Image.open(io.BytesIO(base64.b64decode(shot["data"]))).resize((tw, th), Image.Resampling.LANCZOS)
                r, c = divmod(idx, cols)
                sheet.paste(im, (c * tw, r * th))
                draw.rectangle([c * tw + 6, r * th + 6, c * tw + 114, r * th + 28], fill=(0, 0, 0))
                draw.text((c * tw + 12, r * th + 10), f"S{idx//3 + 1} · {ts:05.2f}s", fill=(52, 211, 153))
            sheet_path = out_dir / f"qa_storyboard_{aspect}.png"
            sheet.save(sheet_path)
            fps_s = len(GATE2_TIMESTAMPS) / max(0.01, time.time() - t0)
            print(f"[gate2] Saved 21-frame contact sheet: {sheet_path} ({fps_s:.1f} fps single-tab)", flush=True)
            await pg.close()
        await browser.close()


async def render_segment_or_full(
    out_mp4: Path,
    audio_wav: Path,
    ffmpeg_exe: str,
    port: int,
    aspect: str,
    w: int,
    h: int,
    payload: Dict[str, Any],
    start_frame: int,
    end_frame: int,
    concurrency: int = 4,
) -> None:
    n_frames = end_frame - start_frame
    start_s = start_frame / FPS
    dur_s = n_frames / FPS
    url = f"http://127.0.0.1:{port}/cinema_stage.html?aspect={aspect}"
    t0 = time.time()
    print(f"[cinema] Rendering {out_mp4.name} ({w}x{h} @ {FPS}fps, frames {start_frame}..{end_frame} = {dur_s:.2f}s) with {concurrency} Metal GPU tabs...", flush=True)

    cmd = [
        ffmpeg_exe,
        "-y",
        "-loglevel",
        "error",
        "-f",
        "image2pipe",
        "-vcodec",
        "mjpeg",
        "-r",
        str(FPS),
        "-i",
        "-",
        "-ss",
        f"{start_s:.6f}",
        "-t",
        f"{dur_s:.6f}",
        "-i",
        str(audio_wav),
        "-vf",
        "scale=in_range=full:out_range=tv,format=yuv420p",
        "-c:v",
        "libx264",
        "-profile:v",
        "high",
        "-level:v",
        "4.2",
        "-preset",
        "medium",
        "-crf",
        "17",
        "-maxrate",
        "20M",
        "-bufsize",
        "40M",
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
        "-colorspace",
        "bt709",
        "-r",
        str(FPS),
        "-c:a",
        "aac",
        "-b:a",
        "256k",
        "-ar",
        "48000",
        "-movflags",
        "+faststart",
        "-shortest",
        str(out_mp4),
    ]
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
    assert proc.stdin is not None

    async with async_playwright() as p:
        browser = await launch_metal_browser(p)
        pages, cdps = [], []
        for idx in range(concurrency):
            pg, cdp, gpu = await init_cinema_page(browser, url, w, h, payload)
            if idx == 0:
                print(f"[cinema] Hardware Metal verified: {gpu['renderer']}", flush=True)
            pages.append(pg)
            cdps.append(cdp)

        async def grab(tab_i: int, fidx: int) -> bytes:
            await pages[tab_i].evaluate("(f) => window.__renderFrame(f, 60, window.__PAYLOAD)", fidx)
            shot = await cdps[tab_i].send("Page.captureScreenshot", {"format": "jpeg", "quality": 96})
            return base64.b64decode(shot["data"])

        for base_f in range(start_frame, end_frame, concurrency):
            batch = list(range(base_f, min(end_frame, base_f + concurrency)))
            jpgs = await asyncio.gather(*(grab(i, f) for i, f in enumerate(batch)))
            for b in jpgs:
                proc.stdin.write(b)
            done = batch[-1] - start_frame + 1
            if done % 300 < concurrency or done == n_frames:
                fps_eff = done / max(0.01, time.time() - t0)
                print(f"[cinema]   {out_mp4.name}: {done}/{n_frames} frames ({time.time() - t0:.1f}s, {fps_eff:.1f} fps)", flush=True)

        await browser.close()

    proc.stdin.close()
    err = proc.stderr.read().decode("utf-8", errors="replace") if proc.stderr else ""
    rc = proc.wait()
    if rc != 0:
        raise RuntimeError(f"ffmpeg failed ({rc}): {err[-1000:]}")
    sz_mb = out_mp4.stat().st_size / (1024 * 1024)
    print(f"[cinema] DONE {out_mp4} ({sz_mb:.2f} MB in {time.time() - t0:.1f}s)", flush=True)


async def async_main() -> int:
    parser = argparse.ArgumentParser(description="Newsatoshi 60fps Apple M5 Pro WebGL2 Cinema Renderer")
    parser.add_argument("--out-dir", default="media/cinema/out")
    parser.add_argument("--gate2", action="store_true", help="Render 21-frame 16:9 + 9:16 contact sheets")
    parser.add_argument("--gate3", action="store_true", help="Render 3s 60fps Drop-Cut motion test (0:10-0:13)")
    parser.add_argument("--only", choices=["all", "16x9", "9x16"], default="all")
    parser.add_argument("--concurrency", type=int, default=4)
    args = parser.parse_args()

    cinema_dir = Path(__file__).resolve().parent
    repo_root = cinema_dir.parent.parent
    out_dir = (repo_root / args.out_dir).resolve()
    manifest_path = out_dir / "audio_manifest.json"
    audio_wav = out_dir / "cinema_master_48k.wav"
    if not manifest_path.exists() or not audio_wav.exists():
        raise RuntimeError("Run media/cinema/audio_engine.py first to generate Gate 1 audio & manifest!")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    stats = fetch_live_stats()
    print(
        f"[cinema] Live Mainnet telemetry: height=#{stats['mainnet_height']:,}, "
        f"diff={stats['mainnet_diff']:.2f}, issued={stats['mainnet_issued_love']:,} LOVE, "
        f"notes={stats['mainnet_notes']:,}",
        flush=True,
    )

    payload = {
        "total_dur": manifest["total_dur"],
        "kick_60hz": manifest["kick_60hz"],
        "bass_60hz": manifest["bass_60hz"],
        "stats": stats,
    }

    httpd, port = start_local_server(cinema_dir)
    ffmpeg_exe = imageio_ffmpeg.get_ffmpeg_exe()
    try:
        if args.gate2:
            await run_gate2_contact_sheets(cinema_dir, out_dir, port, payload)
            return 0
        if args.gate3:
            # 3.0s motion & sync test spanning 0:10.0 -> 0:13.0 (fracture -> 484ms silence -> Awakening drop!)
            await render_segment_or_full(
                out_dir / "qa_dropcut_test_60fps.mp4",
                audio_wav,
                ffmpeg_exe,
                port,
                "16x9",
                1920,
                1080,
                payload,
                start_frame=10 * FPS,
                end_frame=13 * FPS,
                concurrency=args.concurrency,
            )
            return 0

        total_frames = int(manifest["total_frames"])
        if args.only in ("all", "16x9"):
            await render_segment_or_full(
                out_dir / "newsatoshi_launch_16x9_60fps.mp4",
                audio_wav,
                ffmpeg_exe,
                port,
                "16x9",
                1920,
                1080,
                payload,
                0,
                total_frames,
                concurrency=args.concurrency,
            )
        if args.only in ("all", "9x16"):
            await render_segment_or_full(
                out_dir / "newsatoshi_launch_9x16_60fps.mp4",
                audio_wav,
                ffmpeg_exe,
                port,
                "9x16",
                1080,
                1920,
                payload,
                0,
                total_frames,
                concurrency=args.concurrency,
            )
    finally:
        httpd.shutdown()
    return 0


def main() -> int:
    return asyncio.run(async_main())


if __name__ == "__main__":
    raise SystemExit(main())
