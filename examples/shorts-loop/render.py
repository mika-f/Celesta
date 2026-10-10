"""Export SHORTS LOOP to an MP4 whose sound loops as cleanly as its picture.

The exporter's AAC encoder starts from silence, so the first kick of its
soundtrack comes out smeared for about 10 ms, and a player looping the file
plays that smear on every pass. This script keeps the exporter's picture and
re-encodes the score with the loop's last four AAC frames in front of it, so
the encoder starts mid-loop; the MP4's edit list then hides them, as it hides
the encoder's own priming. ffmpeg trims only whole AAC frames this way, so the
lead-in is a whole number of them, and the result is checked to decode to
exactly the score's length.

Requires the built Celesta exporter, ffmpeg, and assets/score.wav
(make-score.py). No Python dependencies.
"""
import argparse
from pathlib import Path
import subprocess
import tempfile
import wave

HERE = Path(__file__).resolve().parent
PREROLL = 4 * 1024  # samples of the loop's end encoded ahead of its start

parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--exporter', default=str(HERE.parents[1] / 'target/release/celesta-exporter'))
parser.add_argument('--overwrite', action='store_true')
args = parser.parse_args()
output = HERE / 'shorts-loop.mp4'
if output.exists() and not args.overwrite:
    parser.error('shorts-loop.mp4 already exists; pass --overwrite to replace it')

with tempfile.TemporaryDirectory(prefix='.render-', dir=HERE) as temporary:
    work = Path(temporary)
    picture = work / 'picture.mp4'
    subprocess.run([args.exporter, '--json', '--react', str(HERE / 'film.tsx'), str(picture)],
                   check=True, stdout=subprocess.DEVNULL)

    with wave.open(str(HERE / 'assets/score.wav')) as score:
        params = score.getparams()
        frames = score.readframes(params.nframes)
    tail = PREROLL * params.nchannels * params.sampwidth
    with wave.open(str(work / 'wrapped.wav'), 'wb') as wrapped:
        wrapped.setparams(params)
        wrapped.writeframes(frames[-tail:] + frames)

    sound = work / 'sound.m4a'
    subprocess.run(['ffmpeg', '-v', 'error', '-i', str(work / 'wrapped.wav'),
                    '-c:a', 'aac', '-b:a', '256k', str(sound)], check=True)
    movie = work / 'shorts-loop.mp4'
    subprocess.run(['ffmpeg', '-v', 'error', '-i', str(picture),
                    '-itsoffset', f'-{PREROLL / params.framerate:.6f}', '-i', str(sound),
                    '-map', '0:v:0', '-map', '1:a:0', '-c', 'copy', '-movflags', '+faststart', str(movie)],
                   check=True)
    decoded = subprocess.run(['ffmpeg', '-v', 'error', '-i', str(movie), '-map', '0:a:0',
                              '-f', 's16le', '-acodec', 'pcm_s16le', '-'], check=True, capture_output=True).stdout
    if len(decoded) != len(frames):
        raise SystemExit(f'the sound decodes to {len(decoded) // (params.nchannels * 2)} samples, '
                         f'not the score\'s {params.nframes}; {output.name} was not written')
    movie.replace(output)
print(f'Finished: {output}', flush=True)
