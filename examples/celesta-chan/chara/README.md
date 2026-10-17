# Celesta-chan's portraits

The portraits were generated with ChatGPT's image generation through the
Codex CLI (0.154.0, model `gpt-6-astra`). Codex was asked to call its image
generation tool and copy the result to a file. The prompts in
[`prompts/`](./prompts) are the ones that were sent, with the scratch output
paths shortened to file names.

| File | What it is |
| --- | --- |
| [`prompts/character.txt`](./prompts/character.txt) | The character settings: look, outfit, colours (taken from the Celesta icon: `#0a0a0a`, `#b5a2e7`, the crescent mark). |
| [`prompts/sheet.txt`](./prompts/sheet.txt) | The prompt for the character sheet, `character.txt` plus the image request. |
| [`sheet.png`](./sheet.png) | The resulting character sheet. Every pose was generated with it attached as the reference. |
| [`prompts/poses.txt`](./prompts/poses.txt) | The pose prompts. Four Codex runs in parallel each received the shared preamble and their own lines. The salute pose (`full_salute.png`) was generated but later dropped from the film. |
| [`prompts/conduct.txt`](./prompts/conduct.txt) | The conducting pose, generated later to replace the salute. |

To generate the character sheet, then a pose from it:

```sh
codex exec -m gpt-6-astra --skip-git-repo-check -s workspace-write \
  -- "$(cat prompts/sheet.txt)" < /dev/null
codex exec -m gpt-6-astra --skip-git-repo-check -s workspace-write \
  --image=sheet.png -- "$(cat prompts/conduct.txt)" < /dev/null
```

Pass the image as `--image=…` and put the prompt after `--`, because
`--image` takes several values and would otherwise swallow the prompt. The
`< /dev/null` stops `codex exec` from waiting for more input on stdin.

## Background removal

Some results came back already transparent. For those on the chroma-key
green, the green was keyed out and its spill removed with ImageMagick. Every
portrait was then trimmed to its visible pixels:

```sh
magick in.png \( +clone -fx "1-min(1,max(0,(g-max(r,b)-0.12)*3))" -colorspace gray \) \
  -alpha off -compose CopyOpacity -composite \
  -channel G -fx "min(g,max(r,b)*1.04+0.02)" +channel keyed.png
magick keyed.png -trim +repage -define png:compression-level=9 out.png
```
