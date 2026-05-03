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

function buildMarkup({ codexLogo, claudeLogo, geminiLogo }) {
  return {
    type: "div",
    props: {
      style: {
        display: "flex",
        position: "relative",
        width: "100%",
        height: "100%",
        backgroundColor: "#f6f7f2",
        color: "#10251f",
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
                "linear-gradient(90deg, rgba(16,37,31,0.08) 1px, transparent 1px), linear-gradient(180deg, rgba(16,37,31,0.08) 1px, transparent 1px)",
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
              border: "2px solid #10251f",
              borderRadius: "8px",
              backgroundColor: "#ffffff",
              padding: "34px 38px",
              boxShadow: "14px 14px 0 #d8b45f",
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
                            type: "div",
                            props: {
                              style: {
                                display: "flex",
                                alignItems: "center",
                                justifyContent: "center",
                                width: "54px",
                                height: "54px",
                                borderRadius: "8px",
                                backgroundColor: "#10251f",
                                color: "#d8b45f",
                                fontSize: "24px",
                                fontWeight: 700,
                              },
                              children: "AH",
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
                          color: "#587067",
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
                          color: "#30453e",
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
                          logoPill("Codex", codexLogo, "#2f7d69", { borderRadius: "6px" }),
                          logoPill("Claude", claudeLogo, "#b7472a"),
                          logoPill("Gemini", geminiLogo, "#315fb5"),
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
                          backgroundColor: "#10251f",
                          color: "#f6f7f2",
                          fontSize: "24px",
                          fontWeight: 700,
                        },
                        children: "Rust-native delivery studio",
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

  const [codexLogo, claudeLogo, geminiLogo] = await Promise.all([
    pngDataUrl(join(rootDir, "src", "assets", "agent-logos", "codex.png")),
    pngDataUrl(join(rootDir, "src", "assets", "agent-logos", "claude.png")),
    rasterizedSvgDataUrl(join(rootDir, "public", "agent-logos", "gemini.svg"), 64),
  ]);

  const svg = await satori(buildMarkup({ codexLogo, claudeLogo, geminiLogo }), {
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
