#!/usr/bin/env bash
# Copies the parts of the ejmlabs design kit this site uses into site/public/design-kit.
# The kit lives at ../ejmlabs/design-kit beside this project, or set DESIGN_KIT. Never edit the copy.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/.." && pwd)"
kit="${DESIGN_KIT:-$(cd "$here/../../ejmlabs/design-kit" && pwd)}"
out="$repo/site/public/design-kit"
rm -rf "$out"
mkdir -p "$out"/{tokens,components,web,fonts,icons,logos,favicon,social,motifs}
cp "$kit/tokens/brand.css" "$out/tokens/"
cp "$kit/components/brand-components.css" "$out/components/"
cp "$kit/web/site.css" "$out/web/"
cp "$kit"/fonts/* "$out/fonts/"
cp "$kit"/icons/*.svg "$out/icons/"
cp "$kit/logos/ejmlabs-paper.svg" "$kit/logos/ejmlabs-primary.svg" "$out/logos/"
cp "$kit"/favicon/* "$out/favicon/"
cp "$kit/social/social-card.png" "$out/social/"
cp "$kit"/motifs/orbit-{forest,paper}.svg "$kit"/motifs/flow-{forest,paper}.svg "$out/motifs/"
echo "Copied the design kit into $out"
