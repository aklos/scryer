const esbuild = require("esbuild");
const { execSync } = require("child_process");
const fs = require("fs");

async function build() {
  try {
    if (fs.existsSync("dist")) {
      fs.rmSync("dist", { recursive: true, force: true });
    }

    // IIFE build for HTML <script> usage
    await esbuild.build({
      entryPoints: ["src/index.ts"],
      bundle: true,
      minify: true,
      keepNames: true,
      platform: "browser",
      format: "iife",
      globalName: "Scryer",
      sourcemap: "inline",
      outfile: "dist/scryer.iife.js",
    });

    // ESM build for imports
    await esbuild.build({
      entryPoints: ["src/index.ts"],
      bundle: true,
      minify: true,
      keepNames: true,
      platform: "browser",
      format: "esm",
      sourcemap: "inline",
      outfile: "dist/scryer.esm.js",
    });

    // Generate TypeScript declarations
    execSync("tsc --declaration --emitDeclarationOnly --outDir dist");

    console.log("✅ Build complete");
  } catch (err) {
    console.error(err);
    process.exit(1);
  }
}

build();
