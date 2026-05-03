import satori from "satori";
import { Resvg } from "@resvg/resvg-js";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  defaultSiteUrl,
  description,
  ogHeadline,
  siteName,
} from "../src/site-meta.js";

const rootDir = dirname(fileURLToPath(new URL("../package.json", import.meta.url)));
const outputDir = join(rootDir, "public", "og");
const outputPath = join(outputDir, "home.png");
const brandHost = new URL(defaultSiteUrl).host;

async function pngDataUrl(path) {
  const buf = await readFile(path);
  return `data:image/png;base64,${buf.toString("base64")}`;
}

async function rasterizedSvgDataUrl(path, size) {
  const svg = await readFile(path, "utf8");
  const resvg = new Resvg(svg, { fitTo: { mode: "width", value: size } });
  const png = resvg.render().asPng();
  return `data:image/png;base64,${png.toString("base64")}`;
}

function logoPill(label, logoSrc, accent, logoStyle = {}) {
  return {
    type: "div",
    props: {
      style: {
        display: "flex",
        alignItems: "center",
        gap: "12px",
        height: "56px",
        padding: "0 20px 0 14px",
        borderRadius: "8px",
        border: `2px solid ${accent}`,
        backgroundColor: "#f9faf4",
        color: "#10251f",
      },
      children: [
        {
          type: "img",
          props: {
            src: logoSrc,
            width: 30,
            height: 30,
            style: { width: "30px", height: "30px", objectFit: "contain", ...logoStyle },
          },
        },
        {
          type: "div",
          props: {
            style: {
              display: "flex",
              fontSize: "28px",
              fontWeight: 700,
              lineHeight: 1,
            },
            children: label,
          },
        },
      ],
    },
  };
}

function buildMarkup({ amonLogo, codexLogo, claudeLogo, geminiLogo }) {
  return {
    type: "div",
    props: {
      style: {
        display: "flex",
        position: "relative",
        width: "100%",
        height: "100%",
        backgroundColor: "#f7f8fb",
        color: "#10131a",
        padding: "54px 64px",
      },
      children: [
        {
          type: "div",
          props: {
            style: {
              display: "flex",
              position: "absolute",
              inset: "0",
              backgroundImage:
                "linear-gradient(90deg, rgba(16,19,26,0.08) 1px, transparent 1px), linear-gradient(180deg, rgba(16,19,26,0.08) 1px, transparent 1px)",
              backgroundSize: "48px 48px",
            },
          },
        },
        {
          type: "div",
          props: {
            style: {
              display: "flex",
              position: "relative",
              flexDirection: "column",
              justifyContent: "space-between",
              width: "100%",
              height: "100%",
              border: "2px solid rgba(16,19,26,0.82)",
              borderRadius: "8px",
              backgroundColor: "#ffffff",
              padding: "34px 38px",
              boxShadow: "14px 14px 0 #11a9a7",
            },
            children: [
              {
                type: "div",
                props: {
                  style: {
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                  },
                  children: [
                    {
                      type: "div",
                      props: {
                        style: {
                          display: "flex",
                          alignItems: "center",
                          gap: "16px",
                        },
                        children: [
                          {
                            type: "img",
                            props: {
                              src: amonLogo,
                              width: 54,
                              height: 54,
                              style: {
                                width: "54px",
                                height: "54px",
                                borderRadius: "8px",
                                objectFit: "contain",
                              },
                            },
                          },
                          {
                            type: "div",
                            props: {
                              style: {
                                display: "flex",
                                fontSize: "34px",
                                fontWeight: 700,
                                lineHeight: 1,
                              },
                              children: siteName,
                            },
                          },
                        ],
                      },
                    },
                    {
                      type: "div",
                      props: {
                        style: {
                          display: "flex",
                          color: "#606a78",
                          fontSize: "23px",
                          fontWeight: 600,
                          lineHeight: 1,
                        },
                        children: brandHost,
                      },
                    },
                  ],
                },
              },
              {
                type: "div",
                props: {
                  style: {
                    display: "flex",
                    flexDirection: "column",
                    gap: "28px",
                  },
                  children: [
                    {
                      type: "div",
                      props: {
                        style: {
                          display: "flex",
                          flexDirection: "column",
                          whiteSpace: "pre-line",
                          fontSize: "88px",
                          fontWeight: 700,
                          lineHeight: 1.02,
                        },
                        children: ogHeadline,
                      },
                    },
                    {
                      type: "div",
                      props: {
                        style: {
                          display: "flex",
                          maxWidth: "930px",
                          color: "#606a78",
                          fontSize: "28px",
                          fontWeight: 500,
                          lineHeight: 1.34,
                        },
                        children: description,
                      },
                    },
                  ],
                },
              },
              {
                type: "div",
                props: {
                  style: {
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center",
                  },
                  children: [
                    {
                      type: "div",
                      props: {
                        style: {
                          display: "flex",
                          gap: "14px",
                        },
                        children: [
                          logoPill("Codex", codexLogo, "#11a9a7", { borderRadius: "6px" }),
                          logoPill("Claude", claudeLogo, "#df5b49"),
                          logoPill("Gemini", geminiLogo, "#3468e7"),
                        ],
                      },
                    },
                    {
                      type: "div",
                      props: {
                        style: {
                          display: "flex",
                          height: "56px",
                          alignItems: "center",
                          padding: "0 20px",
                          borderRadius: "8px",
                          backgroundColor: "#10131a",
                          color: "#ffffff",
                          fontSize: "24px",
                          fontWeight: 700,
                        },
                        children: "Rust-native AI delivery studio",
                      },
                    },
                  ],
                },
              },
            ],
          },
        },
      ],
    },
  };
}

async function main() {
  const [interMedium, interBold] = await Promise.all([
    readFile(join(rootDir, "src", "assets", "fonts", "Inter-500.woff")),
    readFile(join(rootDir, "src", "assets", "fonts", "Inter-700.woff")),
  ]);

  const [amonLogo, codexLogo, claudeLogo, geminiLogo] = await Promise.all([
    rasterizedSvgDataUrl(join(rootDir, "public", "amonhen.svg"), 64),
    pngDataUrl(join(rootDir, "src", "assets", "agent-logos", "codex.png")),
    pngDataUrl(join(rootDir, "src", "assets", "agent-logos", "claude.png")),
    rasterizedSvgDataUrl(join(rootDir, "public", "agent-logos", "gemini.svg"), 64),
  ]);

  const svg = await satori(buildMarkup({ amonLogo, codexLogo, claudeLogo, geminiLogo }), {
    width: 1200,
    height: 630,
    fonts: [
      { name: "Inter", data: interMedium, weight: 500, style: "normal" },
      { name: "Inter", data: interBold, weight: 700, style: "normal" },
    ],
  });

  const resvg = new Resvg(svg, { fitTo: { mode: "width", value: 1200 } });
  const png = resvg.render().asPng();

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, png);
  console.log(`Generated ${outputPath}`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
