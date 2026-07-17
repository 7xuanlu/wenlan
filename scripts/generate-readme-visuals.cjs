#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");

const {
  makeOverview,
  makeLifecycle,
} = require("./readme-product-visuals.cjs");

const ROOT = path.resolve(__dirname, "..");
const ASSET_DIR = path.join(ROOT, "docs", "assets");
const FONT_DIR = path.join(__dirname, "readme-visual-fonts");

const BANNER_VIEWPORTS = {
  desktop: { width: 1280, height: 440 },
  mobile: { width: 720, height: 300 },
};

const BANNER = {
  dark: "#101024",
  white: "#F7F8FF",
  cyan: "#93E3F2",
  latin: 'Arial, "Helvetica Neue", sans-serif',
};

const REQUIRED_BANNER_COPY = [
  "WENLAN",
  "Your source-backed knowledge base,",
  "built to compound.",
];

function esc(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function logoDefs(prefix) {
  return `<linearGradient id="${prefix}-ring" x1="96" y1="256" x2="416" y2="256" gradientUnits="userSpaceOnUse">
      <stop stop-color="#6C63FF"/>
      <stop offset="0.5" stop-color="#5BA3E6"/>
      <stop offset="1" stop-color="#4AC8E8"/>
    </linearGradient>
    <radialGradient id="${prefix}-orb" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(310 144) rotate(52.125) scale(76.0263)">
      <stop stop-color="#FFFFFF"/>
      <stop offset="0.45" stop-color="#A5C4F7"/>
      <stop offset="1" stop-color="#4AC8E8"/>
    </radialGradient>`;
}

function logoMarkup({ x, y, size, prefix }) {
  const scale = size / 512;
  return `<g transform="translate(${x} ${y}) scale(${scale})">
    <rect width="512" height="512" rx="112" fill="#1A1A2E"/>
    <circle cx="256" cy="256" r="160" fill="none" stroke="url(#${prefix}-ring)" stroke-width="76"/>
    <circle cx="322" cy="160" r="42" fill="url(#${prefix}-orb)"/>
  </g>`;
}

function makeBanner(viewport) {
  const { width, height } = BANNER_VIEWPORTS[viewport];
  const mobile = viewport === "mobile";
  const prefix = `banner-${viewport}`;
  const content = mobile
    ? `${logoMarkup({ x: 294, y: 30, size: 132, prefix })}
  <text x="360" y="190" fill="${BANNER.cyan}" font-family="${esc(BANNER.latin)}" font-size="28" font-weight="700" text-anchor="middle" letter-spacing="4">WENLAN</text>
  <text x="360" y="232" fill="${BANNER.white}" font-family="${esc(BANNER.latin)}" font-size="34" font-weight="700" text-anchor="middle">Your source-backed knowledge base,</text>
  <text x="360" y="270" fill="${BANNER.white}" font-family="${esc(BANNER.latin)}" font-size="34" font-weight="700" text-anchor="middle">built to compound.</text>`
    : `${logoMarkup({ x: 144, y: 104, size: 232, prefix })}
  <text x="440" y="174" fill="${BANNER.cyan}" font-family="${esc(BANNER.latin)}" font-size="34" font-weight="700" letter-spacing="4">WENLAN</text>
  <text x="440" y="235" fill="${BANNER.white}" font-family="${esc(BANNER.latin)}" font-size="42" font-weight="700">Your source-backed knowledge base,</text>
  <text x="440" y="283" fill="${BANNER.white}" font-family="${esc(BANNER.latin)}" font-size="42" font-weight="700">built to compound.</text>`;

  const frame = mobile
    ? '<rect x="24" y="16" width="672" height="268" rx="24" fill="#101024" stroke="#2F3769"/>'
    : '<rect x="64" y="28" width="1152" height="384" rx="30" fill="#101024" stroke="#2F3769"/>';

  const svg = `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title desc">
  <title id="title">Wenlan README banner</title>
  <desc id="desc">Wenlan: your source-backed knowledge base, built to compound.</desc>
  <rect width="${width}" height="${height}" fill="${BANNER.dark}"/>
  ${frame}
  ${content}
  <defs>
    ${logoDefs(prefix)}
  </defs>
</svg>
`;

  return {
    group: "banner",
    name: mobile ? "readme-banner-mobile" : "readme-banner",
    width,
    height,
    background: BANNER.dark,
    requiredCopy: REQUIRED_BANNER_COPY,
    svg,
  };
}

function fontData(filename) {
  const file = path.join(FONT_DIR, filename);
  if (!fs.existsSync(file)) {
    throw new Error(`Missing README render font: ${path.relative(ROOT, file)}`);
  }
  return fs.readFileSync(file).toString("base64");
}

let renderFontCss;

function embeddedFonts() {
  if (renderFontCss) return renderFontCss;
  renderFontCss = `
    @font-face {
      font-family: "Fraunces";
      font-style: normal;
      font-weight: 600;
      src: url(data:font/ttf;base64,${fontData("Fraunces-600.ttf")}) format("truetype");
    }
    @font-face {
      font-family: "Instrument Sans";
      font-style: normal;
      font-weight: 400;
      src: url(data:font/ttf;base64,${fontData("InstrumentSans-400.ttf")}) format("truetype");
    }
    @font-face {
      font-family: "Instrument Sans";
      font-style: normal;
      font-weight: 600;
      src: url(data:font/ttf;base64,${fontData("InstrumentSans-600.ttf")}) format("truetype");
    }
    @font-face {
      font-family: "JetBrains Mono";
      font-style: normal;
      font-weight: 500;
      src: url(data:font/ttf;base64,${fontData("JetBrainsMono-500.ttf")}) format("truetype");
    }
  `;
  return renderFontCss;
}

function selectedAssets(only) {
  const banner = [makeBanner("desktop"), makeBanner("mobile")];
  const overview = ["en", "zh-Hans", "zh-Hant"].flatMap((locale) => [
    makeOverview(locale, "desktop"),
    makeOverview(locale, "mobile"),
  ]);
  const lifecycle = ["en", "zh-Hans", "zh-Hant"].flatMap((locale) => [
    makeLifecycle(locale, "desktop"),
    makeLifecycle(locale, "mobile"),
  ]);
  if (only === "banner") return banner;
  if (only === "overview") return overview;
  if (only === "lifecycle") return lifecycle;
  return [...banner, ...overview, ...lifecycle];
}

async function renderSvgToPng(asset, pngPath) {
  const { chromium } = require("playwright");
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext({
      viewport: { width: asset.width, height: asset.height },
      deviceScaleFactor: 1,
    });
    const page = await context.newPage();
    await page.setContent(
      `<style>
        ${embeddedFonts()}
        html, body {
          margin: 0;
          width: ${asset.width}px;
          height: ${asset.height}px;
          overflow: hidden;
        }
        svg { display: block; }
      </style>${asset.svg}`,
      { waitUntil: "load" },
    );
    await page.evaluate(() => document.fonts.ready);
    const layoutErrors = await page.locator("svg").evaluate((svg) => {
      const errors = [];
      const tolerance = 1;
      const view = svg.viewBox.baseVal;
      for (const region of svg.querySelectorAll("[data-fit-region]")) {
        const id = region.getAttribute("data-fit-region");
        const bounds = {
          x: Number(region.getAttribute("data-fit-x")),
          y: Number(region.getAttribute("data-fit-y")),
          width: Number(region.getAttribute("data-fit-width")),
          height: Number(region.getAttribute("data-fit-height")),
        };
        const texts = Array.from(region.querySelectorAll("text")).map((node) => ({
          text: node.textContent || "",
          box: node.getBBox(),
        }));
        for (const item of texts) {
          const { box } = item;
          if (
            box.width <= 0
            || box.height <= 0
            || box.x < bounds.x - tolerance
            || box.y < bounds.y - tolerance
            || box.x + box.width > bounds.x + bounds.width + tolerance
            || box.y + box.height > bounds.y + bounds.height + tolerance
          ) {
            errors.push(
              `${id}: ${JSON.stringify(item.text)} leaves its assigned region `
              + `(text ${box.x.toFixed(1)},${box.y.toFixed(1)},${box.width.toFixed(1)},${box.height.toFixed(1)}; `
              + `region ${bounds.x},${bounds.y},${bounds.width},${bounds.height})`,
            );
          }
        }
        if (region.getAttribute("data-check-overlap") !== "false") {
          for (let left = 0; left < texts.length; left += 1) {
            for (let right = left + 1; right < texts.length; right += 1) {
              const a = texts[left].box;
              const b = texts[right].box;
              const overlapX = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
              const overlapY = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
              if (overlapX > tolerance && overlapY > tolerance) {
                errors.push(
                  `${id}: ${JSON.stringify(texts[left].text)} overlaps ${JSON.stringify(texts[right].text)}`,
                );
              }
            }
          }
        }
      }
      for (const node of svg.querySelectorAll("text")) {
        const box = node.getBBox();
        if (
          box.x < -tolerance
          || box.y < -tolerance
          || box.x + box.width > view.width + tolerance
          || box.y + box.height > view.height + tolerance
        ) {
          errors.push(`viewport: ${JSON.stringify(node.textContent || "")} is clipped`);
        }
      }
      return errors;
    });
    if (layoutErrors.length > 0) {
      throw new Error(`${asset.name} layout check failed:\n- ${layoutErrors.join("\n- ")}`);
    }
    await page.locator("svg").screenshot({ path: pngPath, omitBackground: false });
    await context.close();
  } finally {
    await browser.close();
  }
}

async function writeAsset(asset) {
  fs.mkdirSync(ASSET_DIR, { recursive: true });
  const svgPath = path.join(ASSET_DIR, `${asset.name}.svg`);
  const pngPath = path.join(ASSET_DIR, `${asset.name}.png`);
  fs.writeFileSync(svgPath, asset.svg, "utf8");
  await renderSvgToPng(asset, pngPath);
}

function parseHex(hex) {
  const value = hex.replace("#", "");
  return [0, 2, 4].map((index) => Number.parseInt(value.slice(index, index + 2), 16));
}

async function checkPng(asset, pngPath) {
  const sharp = require("sharp");
  const errors = [];
  if (!fs.existsSync(pngPath)) {
    return [`missing ${path.relative(ROOT, pngPath)}`];
  }
  const image = sharp(pngPath);
  const metadata = await image.metadata();
  if (metadata.width !== asset.width || metadata.height !== asset.height) {
    errors.push(
      `${path.relative(ROOT, pngPath)} is ${metadata.width}x${metadata.height}; `
      + `expected ${asset.width}x${asset.height}`,
    );
    return errors;
  }
  const { data, info } = await image.removeAlpha().raw().toBuffer({ resolveWithObject: true });
  const expected = parseHex(asset.background);
  const points = [
    [0, 0],
    [info.width - 1, 0],
    [0, info.height - 1],
    [info.width - 1, info.height - 1],
  ];
  for (const [x, y] of points) {
    const offset = (y * info.width + x) * info.channels;
    const actual = Array.from(data.subarray(offset, offset + 3));
    if (actual.some((channel, index) => channel !== expected[index])) {
      errors.push(
        `${path.relative(ROOT, pngPath)} corner ${x},${y} is ${actual.join(",")}; `
        + `expected ${expected.join(",")}`,
      );
    }
  }
  return errors;
}

function compactVisibleSvgText(svg) {
  return svg
    .replace(/<[^>]*>/g, "")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/\s+/gu, "");
}

async function checkAsset(asset) {
  const errors = [];
  const svgPath = path.join(ASSET_DIR, `${asset.name}.svg`);
  const pngPath = path.join(ASSET_DIR, `${asset.name}.png`);
  if (!fs.existsSync(svgPath)) {
    errors.push(`missing ${path.relative(ROOT, svgPath)}`);
  } else {
    const current = fs.readFileSync(svgPath, "utf8");
    if (current !== asset.svg) {
      errors.push(`${path.relative(ROOT, svgPath)} is not current`);
    }
    const visibleText = compactVisibleSvgText(current);
    for (const required of asset.requiredCopy) {
      if (!visibleText.includes(compactVisibleSvgText(required))) {
        errors.push(`${path.relative(ROOT, svgPath)} is missing ${JSON.stringify(required)}`);
      }
    }
  }
  errors.push(...(await checkPng(asset, pngPath)));
  return errors;
}

function parseArgs(argv) {
  const mode = argv.includes("--write") ? "write" : argv.includes("--check") ? "check" : null;
  const onlyIndex = argv.indexOf("--only");
  const only = onlyIndex >= 0 ? argv[onlyIndex + 1] : "all";
  if (!mode) {
    throw new Error("Use --write or --check");
  }
  if (!["all", "banner", "overview", "lifecycle"].includes(only)) {
    throw new Error(`Unknown --only value: ${only}`);
  }
  return { mode, only };
}

async function main() {
  const { mode, only } = parseArgs(process.argv.slice(2));
  const assets = selectedAssets(only);
  if (mode === "write") {
    for (const asset of assets) {
      await writeAsset(asset);
    }
    console.log(`${assets.length * 2} assets generated`);
    return;
  }

  const errors = [];
  for (const asset of assets) {
    errors.push(...(await checkAsset(asset)));
  }
  if (errors.length > 0) {
    for (const error of errors) {
      console.error(`- ${error}`);
    }
    process.exitCode = 1;
    return;
  }
  console.log(`${only} assets are current`);
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exitCode = 1;
});
