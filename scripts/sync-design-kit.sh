#!/usr/bin/env bash
# Copies the parts of the ejmlabs design kit this site uses into site/public/design-kit.
# The kit lives at ../ejmlabs/design-kit beside this project, or set DESIGN_KIT. Never edit the copy.
# Files that the style sheets load by url() are found by reading the style sheets, so new motifs and icons follow along.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/.." && pwd)"
kit="${DESIGN_KIT:-$(cd "$here/../../ejmlabs/design-kit" && pwd)}"
out="$repo/site/public/design-kit"
rm -rf "$out"
copy() { mkdir -p "$out/$(dirname "$1")"; cp "$kit/$1" "$out/$1"; }
for file in tokens/brand.css components/brand-components.css web/site.css ui/theme.css web/theme.js web/motion.js web/analytics.js \
  social/social-card.png fonts/Manrope-Variable.ttf fonts/Manrope-OFL.txt fonts/IBMPlexMono-Medium.ttf fonts/IBMPlexMono-OFL.txt; do
  copy "$file"
done
for file in "$kit"/favicon/*; do copy "favicon/$(basename "$file")"; done
for css in tokens/brand.css components/brand-components.css web/site.css ui/theme.css; do
  grep -ohE "url\(['\"]?\.\./[^'\")#]+" "$kit/$css" | sed -E "s/url\(['\"]?\.\.\///" || true
done | sort -u | while read -r file; do [ -f "$kit/$file" ] && copy "$file"; done
echo "Copied the design kit into $out"
