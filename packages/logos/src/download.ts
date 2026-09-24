import { concepts, iconSvg, palettes } from "./icons";

function saveBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function downloadIcon(
  format: "svg" | "png",
  concept: number,
  palette: number,
  tile: boolean,
) {
  const filename = `celesta-${concepts[concept].name.toLowerCase().replaceAll(" ", "-")}-${palettes[palette].label}-${tile ? "icon" : "mark"}.${format}`;
  const svg = iconSvg(concept, palette, tile).replace(
    'width="256" height="256"',
    'width="1024" height="1024"',
  );
  const blob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
  if (format === "svg") saveBlob(blob, filename);
  else {
    const url = URL.createObjectURL(blob);
    try {
      const image = new Image();
      image.src = url;
      await image.decode();
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = 1024;
      const context = canvas.getContext("2d");
      if (!context) throw new Error("Canvas unavailable");
      context.drawImage(image, 0, 0, 1024, 1024);
      const png = await new Promise<Blob>((resolve, reject) =>
        canvas.toBlob(
          (result) =>
            result ? resolve(result) : reject(new Error("PNG export failed")),
          "image/png",
        ),
      );
      saveBlob(png, filename);
    } finally {
      URL.revokeObjectURL(url);
    }
  }
}
