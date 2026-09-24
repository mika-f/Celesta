import {
  StrictMode,
  useId,
  useMemo,
  useState,
  type CSSProperties,
} from "react";
import { createRoot } from "react-dom/client";
import "./style.css";
import { concepts, iconSvg, palettes } from "./icons";
import { downloadIcon } from "./download";

function Icon({
  concept = 0,
  palette = 0,
  tile = true,
}: {
  concept?: number;
  palette?: number;
  tile?: boolean;
}) {
  const id = useId();
  const svg = useMemo(
    () => iconSvg(concept, palette, tile, id),
    [concept, palette, tile, id],
  );
  // The markup is generated exclusively from our local SVG definitions and palettes.
  return (
    <span
      style={{ display: "contents" }}
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}

function App() {
  const [selectedConcept, setSelectedConcept] = useState(0);
  const [selectedPalette, setSelectedPalette] = useState(0);
  const [background, setBackground] = useState("light");
  const [tile, setTile] = useState(true);
  const [exportStatus, setExportStatus] = useState("");
  const [exporting, setExporting] = useState(false);

  async function download(format: "svg" | "png") {
    setExporting(true);
    setExportStatus("書き出しています…");
    try {
      await downloadIcon(format, selectedConcept, selectedPalette, tile);
      setExportStatus(`${format.toUpperCase()} を書き出しました。`);
    } catch {
      setExportStatus("書き出しに失敗しました。もう一度お試しください。");
    } finally {
      setExporting(false);
    }
  }

  return (
    <div className="page-shell mx-auto">
      <header className="flex items-center justify-between">
        <a
          className="brand flex items-center gap-2.5"
          href="#"
          aria-label="Celesta ホーム"
        >
          <span className="brand-icon">
            <Icon />
          </span>
          <span>
            celesta<span className="brand-period">.</span>
          </span>
        </a>
        <span className="header-label hidden sm:block">
          A LITTLE SPARK, A NEW STORY.
        </span>
        <a href="#atelier" className="nav-link flex items-center gap-2">
          Icon atelier <span aria-hidden="true">↗</span>
        </a>
      </header>
      <main>
        <section className="intro relative">
          <div className="eyebrow flex items-center gap-2">
            <span className="tiny-star">✦</span> CELESTA ICON EXPLORATIONS{" "}
            <span className="edition">VOL. 01</span>
          </div>
          <h1>
            A little spark.
            <br />A world of <em>stories.</em>
            <span className="heading-star" aria-hidden="true">
              ✧
            </span>
          </h1>
          <div className="intro-bottom flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
            <p>
              ひらめきに、かわいいかたちを。
              <br />
              <span>動画編集ソフト Celesta の、小さなアイコン工房。</span>
            </p>
            <span className="handwritten" aria-hidden="true">
              made for your imagination <span>↙</span>
            </span>
          </div>
        </section>
        <section
          id="atelier"
          aria-label="アイコン工房"
          className="atelier grid"
        >
          <div
            className="preview-panel relative flex flex-col"
            data-background={background}
            style={
              {
                "--preview-wash": palettes[selectedPalette].wash,
              } as CSSProperties
            }
          >
            <div className="preview-top flex items-center justify-between">
              <span className="panel-label">THE LITTLE BIG IDEA</span>
              <span className="preview-tag">APP ICON</span>
            </div>
            <div className="hero-art flex flex-1 flex-col items-center justify-center">
              <div id="hero-icon" className="hero-icon">
                <Icon
                  concept={selectedConcept}
                  palette={selectedPalette}
                  tile={tile}
                />
              </div>
              <span className="preview-wordmark">
                celesta<span>.</span>
              </span>
              <span className="preview-caption">Make something wonderful.</span>
            </div>
            <div className="preview-bottom flex items-center justify-between">
              <span className="panel-label">DESIGNED TO SPARK JOY</span>
              <div
                className="backgrounds flex gap-1"
                role="group"
                aria-label="プレビュー背景"
              >
                <button
                  data-bg="light"
                  aria-label="ライト背景"
                  aria-pressed={background === "light"}
                  onClick={() => setBackground("light")}
                >
                  <span></span>
                </button>
                <button
                  data-bg="dark"
                  aria-label="ダーク背景"
                  aria-pressed={background === "dark"}
                  onClick={() => setBackground("dark")}
                >
                  <span></span>
                </button>
                <button
                  data-bg="grid"
                  aria-label="透明背景"
                  aria-pressed={background === "grid"}
                  onClick={() => setBackground("grid")}
                >
                  <span></span>
                </button>
              </div>
            </div>
          </div>
          <aside className="controls">
            <div className="flex items-center justify-between">
              <span className="eyebrow">MEET YOUR ICON</span>
              <span
                id="concept-count"
                className="muted mono"
              >{`0${selectedConcept + 1} / 03`}</span>
            </div>
            <h2 id="concept-name">{concepts[selectedConcept].name}</h2>
            <p id="concept-subtitle" className="concept-subtitle">
              {concepts[selectedConcept].subtitle}
            </p>
            <p id="concept-description" className="concept-description">
              {concepts[selectedConcept].description}
            </p>
            <div className="control-section">
              <div className="flex items-center justify-between">
                <h3>Color palette</h3>
                <span id="palette-name" className="muted">
                  {palettes[selectedPalette].name}
                </span>
              </div>
              <div
                className="swatches flex gap-3"
                role="group"
                aria-label="アイコンの色"
              >
                {palettes.map((p, i) => (
                  <button
                    key={p.name}
                    className="swatch"
                    data-palette={i}
                    aria-label={p.label}
                    aria-pressed={i === selectedPalette}
                    style={{ "--swatch": p.mid } as CSSProperties}
                    onClick={() => setSelectedPalette(i)}
                  >
                    <span />
                  </button>
                ))}
              </div>
            </div>
            <div className="control-section appearance flex items-center justify-between">
              <div>
                <h3>App icon background</h3>
                <p className="muted">角丸の背景をつける</p>
              </div>
              <button
                id="tile-toggle"
                className="toggle"
                role="switch"
                aria-checked={tile}
                aria-label="角丸の背景"
                onClick={() => setTile((value) => !value)}
              >
                <span></span>
              </button>
            </div>
            <div className="download-area">
              <button
                id="download-svg"
                disabled={exporting}
                onClick={() => void download("svg")}
                className="primary-button flex items-center justify-center gap-2"
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  aria-hidden="true"
                >
                  <path
                    d="M12 3v12m-5-5 5 5 5-5M5 16v5h14v-5"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>{" "}
                Download SVG
              </button>
              <button
                id="download-png"
                disabled={exporting}
                onClick={() => void download("png")}
                className="secondary-button flex items-center justify-center gap-2"
              >
                Download PNG <span>1024 × 1024</span>
              </button>
              <p className="export-note">
                小さなアイコンも、大きなひらめきも。
              </p>
              <p id="export-status" role="status" className="export-status">
                {exportStatus}
              </p>
            </div>
          </aside>
        </section>
        <section
          className="concepts-section"
          aria-labelledby="concepts-heading"
        >
          <div className="section-heading flex items-baseline justify-between">
            <h2 id="concepts-heading">
              Three little personalities<span>.</span>
            </h2>
            <span className="muted hidden sm:block">
              お気に入りのかたちを選んで
            </span>
          </div>
          <div className="concept-grid grid grid-cols-1 sm:grid-cols-3 gap-4">
            {concepts.map((c, i) => (
              <button
                key={c.name}
                className="concept-card flex items-center text-left"
                data-concept={i}
                aria-pressed={i === selectedConcept}
                onClick={() => setSelectedConcept(i)}
              >
                <span className="concept-thumbnail">
                  <Icon concept={i} palette={selectedPalette} />
                </span>
                <span className="flex-1">
                  <span className="concept-number">0{i + 1}</span>
                  <span className="concept-title">{c.name}</span>
                  <span className="concept-detail">
                    {["星のきらめき", "月のリボン", "ひらめきの花"][i]}
                  </span>
                </span>
                <span className="selection-mark" aria-hidden="true">
                  {i === selectedConcept ? "✓" : "↗"}
                </span>
              </button>
            ))}
          </div>
        </section>
        <section
          className="detail-strip grid"
          aria-label="サイズプレビューとデザインコンセプト"
        >
          <div className="size-preview">
            <span className="eyebrow">SMALL SIZE, SAME MAGIC.</span>
            <div id="size-icons" className="flex items-end">
              {[64, 48, 32, 16].map((size) => (
                <div key={size} className="size-sample">
                  <div style={{ width: size, height: size }}>
                    <Icon
                      concept={selectedConcept}
                      palette={selectedPalette}
                      tile={tile}
                    />
                  </div>
                  <span>{size}</span>
                </div>
              ))}
            </div>
          </div>
          <div className="design-note flex items-center gap-5">
            <span className="note-spark" aria-hidden="true">
              ✳
            </span>
            <div>
              <h3>創作に、ひとさじのときめき。</h3>
              <p>
                やさしい色、まるいかたち、きらりと光る遊び心。
                <br />
                開くたびに、つくりたくなるアイコンを。
              </p>
            </div>
          </div>
        </section>
      </main>
      <footer className="flex items-center justify-between">
        <span>
          celesta <span className="muted">/ icon atelier</span>
        </span>
        <span className="muted">
          Crafted with a little stardust. <span className="footer-star">✦</span>
        </span>
      </footer>
    </div>
  );
}

createRoot(document.getElementById("app")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
