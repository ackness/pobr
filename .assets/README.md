# PoBR promotional images

| File | Use | Size |
| --- | --- | --- |
| [pobr-preview-en.jpg](pobr-preview-en.jpg) | English README and GitHub social preview | 1280 × 640 |
| [pobr-preview-zh-CN.jpg](pobr-preview-zh-CN.jpg) | Simplified Chinese README | 1280 × 640 |

Both files are opaque JPEGs under 1 MB. The README banners link to the live app.

To set the repository preview, open **Settings → General → Social preview → Edit → Upload an image** and select `pobr-preview-en.jpg`. Committing a README image does not configure this repository setting. The preview has not been uploaded by this change.

[GitHub's official guidance](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/customizing-your-repositorys-social-media-preview) recommends 1280 × 640 pixels and requires PNG, JPG or GIF under 1 MB (checked 2026-10-12).

## Creative direction and sources

Revision 2, generated with the built-in imagegen tool on 2026-10-12. This replaces the earlier gold constellation concept with a Ranger in an atmospheric forest, based on official Path of Exile 2 visual references. The introduction identifies the browser build planner, equipment/support-gem comparisons and passive upgrades. Typography uses ivory and pale green with no decorative frame.

The references were inspected on the [official Path of Exile 2 Early Access page](https://pathofexile2.com/early-access):

- [Ranger class artwork](https://web.poecdn.com/protected/image/poe2/early-access/character-classes/backgrounds/ranger.webp?key=xooV4yBK-TYq4gSSgKcBLg): primary character, clothing, bow and painterly material reference.
- [Wraeclast forest artwork](https://pathofexile2.com/protected/image/poe2/early-access/world/wraeclast.webp?key=dkKsxFCQhf_KBiO66Vibjg): forest, shadows, mist and environmental depth.
- [Skill-system combat screenshot](https://pathofexile2.com/protected/image/poe2/early-access/skill-system.webp?key=waS9SQNxzq_xG8B5xv9rog): restrained blue lightning and game atmosphere.

Reference artwork and Path of Exile assets belong to Grinding Gear Games. These banners are AI-generated adaptations for an independent community project, not official GGG artwork or a product endorsement. The project's code license does not grant rights to the underlying third-party artwork. Downloaded reference files are local working inputs and are not included in the repository.

“Lightweight” describes browser access without a desktop installation. The banners make no quantified download-size, memory or speedup claim; the README documents measured performance and its limits. Existing screenshots and beta/coverage notes remain in both READMEs.

The English artwork was generated first, then given the requested two-line feature introduction. The Chinese version localizes that finished image. Final generated PNGs were exported with macOS `sips` as JPEGs at quality 90, resizing from 1774 × 887 to 1280 × 640 without cropping or changing the aspect ratio.

## English generation prompt

Inputs, in order: Ranger class artwork, Wraeclast forest artwork, skill-system combat screenshot.

```text
Use case: ads-marketing
Asset type: Finished 2:1 landscape GitHub social preview / README banner for PoBR, a lightweight fast open-source Path of Exile 2 build planner.
Primary request: Create a completely new, art-directed promotional image by combining the supplied OFFICIAL PATH OF EXILE 2 VISUAL REFERENCES with clean, contemporary software-product typography. It must immediately feel connected to the actual game.
Input images:
1. Official Ranger class painting: PRIMARY character and art-direction reference. Use its recognizable short-haired female Ranger, practical weathered green leather armour, large organic recurved bow, gnarled trees and painterly materials. Recompose the Ranger into the right half; a confident, poised archer preparing the next arrow. Preserve the grounded, rugged character design.
2. Official Wraeclast forest painting: environment, mist, depth, shadow and cool moonlight reference.
3. Official skill-system combat screenshot: only a subtle reference for restrained blue lightning accents and actual game atmosphere; do not reproduce the screenshot or UI.
Composition: exact 2:1 landscape, ideally 2560x1280, ultimately 1280x640. The background is a single continuous cinematic painting of the ancient forest, with tall ruined stone silhouettes behind the branches. Right 58%: the Ranger shown from head to below waist, bow clearly visible, beautifully lit from behind by pale green-blue mist. Her face is sharp, expressive, mature, human, and believable. Slight foreground leaves and fine atmospheric dust give depth. Left 42%: quiet dark ink-green forest shadow providing generous unobstructed space for type. The two halves blend naturally through fog and branches; no dividing line or cards. Keep essential text 64px from edges at final size. A few tiny dim skill-tree connections near the lower right can hint at planning, but remain extremely subtle, flat, and subordinate to the character.
Art direction: premium Path of Exile 2 class illustration and tasteful developer-tool brand. Painterly realism, detailed worn leather, natural branches, deep forest green, desaturated teal, steel blue, bone-white text, muted silver. Atmospheric and beautifully composed, with a clear focal point. Restrained highlights, very little gold. Character artwork should feel as accomplished as the references.
Typography: clean elegant contemporary sans-serif, precisely spaced; no bevels, no decorative fantasy font. The product name is the only large text. Comfortable white space and a small number of coherent text blocks.
Render EXACTLY these five blocks, no extra words:
1. "PoBR" — left upper-middle, very large clean warm-ivory wordmark, about 100px at 1280px width. EXACT case P o B R. No serif slab or ornament.
2. "PATH OF EXILE 2" followed by a line break and "BUILD PLANNER" — directly below the name, refined tracked medium-weight text around 24px at final size.
3. "Lightweight. Fast." — below that with clear separation, around 29px in pale sage green, one single line.
4. "Runs in your browser." — supporting line below the benefit, approximately 21px, warm pale gray.
5. "Rust + WebAssembly  /  Open source" — small but readable bottom-left footer, approximately 17px.
And a discreet footer URL "pobr-web.pages.dev" aligned at bottom-right.
Avoid: gold decorative frames, giant shiny jewels, orreries, abstract node-ball sculpture, generic 3D technology backgrounds, ornate dividers, full-screen webs of glowing lines, blue holographic HUD panels, crowded feature lists, big duplicated headline, excessive bloom, extreme saturation, cartoon/anime appearance, glamour model aesthetics, blood, severed heads, monsters attacking the viewer. Do not put a Path of Exile logo, official endorsement, or GGG logo on the banner. No performance numbers or unsupported accuracy claims. Opaque edge-to-edge artwork. Do not borrow the composition or typography from any earlier PoBR banner.
```

## Feature introduction prompt

Input: the first generated English image.

```text
Use case: precise-object-edit
Input image 1: edit target, the finished PoBR Ranger/forest banner.
The user wants slightly more introductory information about what the tool does. Keep the whole artwork, exact 2:1 aspect ratio, character, face, bow, colors, layout and all existing typography unchanged. Make ONE localized addition in the quiet dark area below "Runs in your browser." and above the footer:
Add exactly these two lines, left-aligned with the existing main text, in readable warm-gray regular sans-serif around 19px at final 1280px width:
"Compare gear and support gems."
"Plan your next passive upgrades."
Maintain generous line spacing, and ensure the text does not overlap the character or the bottom footer. The two new lines should be subordinate in size to "Runs in your browser." but clearly readable at README width. Do not move or resize the PoBR wordmark. Do not change anything else. Retain all punctuation verbatim. No extra decorations. Fully opaque image, same exact 2:1 framing.
```

## Chinese localization prompt

Input: the English image with the feature introduction.

```text
Use case: text-localization
Input image 1: edit target, the finished English PoBR promotional banner featuring the Ranger with bow in a misty forest.
Create the matching Simplified Chinese edition. Change ONLY the language of the text. Preserve the whole image, exact 2:1 framing, character identity, face, hands, bow, trees, atmospheric depth, cool green-blue lighting, typography regions and hierarchy unchanged.
Use refined modern Simplified Chinese sans-serif typography with clean natural glyphs, excellent spacing, and sizes harmonizing with the English version. Keep all main text left-aligned and readable. Preserve the pale sage color for the benefit line. Do not inflate the Chinese text to fill all available width; keep elegant negative space.
Exact replacements:
- Keep "PoBR" identical.
- Replace the two-line descriptor "PATH OF EXILE 2" / "BUILD PLANNER" with two lines: "流放之路 2" / "BD 规划工具".
- Replace "Lightweight. Fast." with "轻量 · 快速".
- Replace "Runs in your browser." with "浏览器即开即用".
- Replace "Compare gear and support gems." with "比较装备与辅助宝石".
- Replace "Plan your next passive upgrades." with "规划下一步天赋升级".
- Replace "Rust + WebAssembly  /  Open source" with "Rust + WebAssembly  /  开源".
- Keep "pobr-web.pages.dev" identical.
Use exactly these Chinese characters without any added punctuation. Keep all the English source words removed wherever translated. No duplicated text, no added badges, frames or decorative effects. Fully opaque, finished 2:1 banner.
```
