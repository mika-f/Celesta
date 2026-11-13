"""Render bounded batches, then join pictures and mux the original score.

Requires the built Celesta exporter plus ffmpeg. No Python dependencies.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--exporter', default=str(HERE.parents[1] / 'target/release/celesta-exporter'))
parser.add_argument('--overwrite', action='store_true')
args = parser.parse_args()
output = HERE / 'afterimage.mp4'
if output.exists() and not args.overwrite:
    parser.error('afterimage.mp4 already exists; pass --overwrite to replace it')

with tempfile.TemporaryDirectory(prefix='.render-', dir=HERE) as temporary:
    work = Path(temporary)

    def render(second):
        destination = work / f'{second:02}.mp4'
        result = subprocess.run([
            args.exporter, '--react', str(HERE / 'film.tsx'),
            '--from', str(second), '--to', str(second + 1),
            '--video-encoder', 'ffmpeg-x264/libx264',
            '--video-option', 'preset=medium', '--video-option', 'crf=17',
            str(destination),
        ], capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(f'Second {second}: {result.stderr}')
        # Drop each chunk's AAC priming before concat; the full score is
        # muxed only once below. Video-only chunks start at exactly zero.
        picture = work / f'{second:02}-picture.mp4'
        subprocess.run([
            'ffmpeg', '-v', 'error', '-i', str(destination), '-map', '0:v:0',
            '-c:v', 'copy', str(picture),
        ], check=True)
        print(f'Rendered seconds {second:02}–{second + 1:02}', flush=True)
        return picture

    with ThreadPoolExecutor(max_workers=3) as pool:
        clips = list(pool.map(render, range(24)))
    concat = work / 'frames.txt'
    concat.write_text(''.join(f"file '{clip.name}'\n" for clip in clips))
    movie = work / 'afterimage.mp4'
    subprocess.run([
        'ffmpeg', '-v', 'error', '-f', 'concat', '-safe', '0', '-i', str(concat),
        '-i', str(HERE / 'assets/score.wav'), '-map', '0:v:0', '-map', '1:a:0',
        '-c:v', 'copy', '-c:a', 'aac', '-b:a', '256k', '-t', '24',
        '-movflags', '+faststart', str(movie),
    ], check=True)
    movie.replace(output)
print(f'Finished: {output}', flush=True)
