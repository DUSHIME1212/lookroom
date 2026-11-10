# How the original's presets work

Read from the decompiled program, version 2.27.0. Addresses are in the main
program. "Estimated" means I looked in the code and did not find the value, so
it is a choice made for this desktop version.

## The preset strip (`PresetsView`)

| What | Value | Where |
| --- | --- | --- |
| Layout | Sideways scrolling row, scroll bar hidden, grouped by category | 0x10059a618, 0x10059ac84 |
| Space under the row | 20 | 0x10059a618 |
| Panel height in preset mode | 0.2 of the editor's height, kept between 150 and 250 | 0x1001aaab4, constant 0x1011a7bf8 |
| Saved panel height | A cached height above 0 replaces the rule; this app has no resizing, so it is not used | 0x1001aad58 |
| Panel padding | 16 at the top and sides (`pad` 8, doubled) | 0x1001bd9f4, 0x1001b7608 |
| Tile side | What remains of the panel height after padding, title and the space under the row | follows from the rows above |
| Tile shape | Square (aspect ratio 1) | 0x10059b8d8 |
| Tile corner radius | 4 | 0x10059b8d8 |
| Picture share of tile height | 0.8 | 0x10059d61c (constant at 0x1011a8990) |
| Tile content | The person's own photo with that preset applied | 0x10059e074 (`sourceImageUIImage`) |
| Caption text | Caption size, semibold, black | 0x10059e2e0 |
| Caption background | White; accent colour when selected | 0x10059e2e0 |
| Caption padding | 4 on all sides | 0x10059e2e0 |
| Caption shrink limit | 0.5 of its size | 0x10059e2e0 |
| Dimmed tile opacity | 0.6 when a state flag is set; what sets it was not determined, so this app dims a tile while it is held | 0x10059d61c |
| Between groups | White line 1 wide, opacity 0.4, not after the last group | 0x10059b184 |
| Interaction | One tap applies; long press opens the menu | 0x10059b8d8 |
| Menu items | Duplicate, Rename Preset, Update Using Current | 0x10059f6ec, 0x10059ac84, app wording |
| Panel title | "Filter Presets", size 14, bold, white | 0x1001bb2d0 |
| Your presets come first | Group `0User Filters`; the 0 sorts it to the front | 0x1005ee2d0 |
| Saved in | `UserPresets/userFilter.json` | 0x1005569f0, 0x1005ee2d0 |
| Built-in catalogue | JSON files in `DefaultLuts.bundle` (Cinema, DeepLooks) | 0x100593304, 0x1005966d4 |

## Tool row

Measured from the five reference screens supplied for this version.

| What | Value |
| --- | --- |
| Tool order | Sliders, film, prism, layers, theatre masks (presets), crop, cube |
| Accent blue | #0C9FFF, sampled from the selected tab and tool |
| Panel fill | #161618; the reference panels sit around #181818 |
| Button fill | #2C2C2E |
| Layout | Photo, tool panel, tab row, tool capsule, top to bottom. From 900 points wide the panels move to a column beside the photo |
| Icons | Hugeicons free set 4.4.0, MIT licence, in `assets/hugeicons`. Film, prism and cube carry a colour run, as in the reference |

## Sliders

| What | Value | Where |
| --- | --- | --- |
| Temperature range | -3000 to 3000 | 0x10055480c |
| Temperature readout | value + 6500, shown as kelvin | 0x100553e94 |
| Contrast meaning | Range 0 to 2 at 0x10055480c, but the exact shader formula is not read | 0x10055480c |
| Tint range | -100 to 100 | 0x10055480c |
| 0 to 2 sliders (saturation and contrast use this range) | 0 to 2 | 0x10055480c |
| Saved preset fields | exposure, contrast, brightness, highlight, shadow, whites, blacks, gamma, saturation, vibrance, hue, temp, tint, fade, clarity, sharpness, vignette, grain, halation, bloom, colorGrade, luts | preset JSON in `DefaultLuts.bundle` |
| Halation defaults | threshold 0.7, glow 50 | preset JSON |
| Grain defaults | size 1, roughness 1 | preset JSON |

## Colour tables

| What | Value | Where |
| --- | --- | --- |
| Size of the built-in tables | 33 (`LUT_3D_SIZE 33`) | `.cube` files in `DefaultLuts.bundle` |
| Export sizes | 33 and 64 | app wording "Cube 33", "Cube 64" |
| Table strength | Blends original and table result | 0x10008d5a8 |
| Stacking | Several tables run in order; Cinema presets stack three | preset JSON |
| Loading | `colorCubeFilterFromLUT` in `CubeLoader.swift` | 0x10050b764 |

## Order of the colour steps

The first version of this order was read from the master render function
0x100f45c74. Steps that have been added since sit where that order puts them.
Steps marked "this app's choice" had no place in it.

1. Color Space Transform (this app's choice: it has to come first)
2. Shot Match
3. White balance and hue
4. Exposure
5. Whites and blacks
6. Highlights and shadows
7. Dehaze
8. Contrast, brightness, gamma
9. Clarity, structure
10. Film contrast, shadow toe, highlight shoulder
11. RGB curves, then the six hue and saturation curves
12. Color Wheels, then HDR Grade
13. Color grade (Tone Wheels)
14. Vibrance, saturation
15. Channel Mixer, Color Slice, HSL Mixer, Color Luminance, Tetra, Color Separation
16. Film density, film saturation
17. Film Look, Bleach Bypass, Saturate, Technicolor, Foliage, Filmic Split, Super 8 colour
18. LUTs, in stack order
19. Conversion out of DaVinci Wide Gamut, when that workflow is on
20. Diffusion
21. Bloom
22. Softness from Film Look and Super 8
23. Sharpness
24. Fade
25. Lens Distortion and Chromatic Aberration
26. Gate weave (video only, never on frame 0)
27. CRT Emulation
28. Grain
29. Halation
30. Vignette
31. Dust, flicker, Textures (this app's choice)
32. Print Simulation
33. The Super 8 gate

An exported .cube holds only steps that depend on one pixel alone: 1 to 19, Fade
and Print Simulation. Anything that looks at neighbouring pixels can't live in a LUT.

## Estimated

| What | Value here |
| --- | --- |
| Title row height | 17 (the font size is known, its line height is not stored) |
| Gap under the title | 8 (the code uses SwiftUI's default spacing) |
| Gap between tiles | 8 (the code uses SwiftUI's default spacing) |
| Inset of the group line | 16 at each end |
| Order inside colour grade | Highlights, midtones, shadows. Four bands run in a row, but they are told apart by codes I could not tie to names |
| Gamma position | Right after brightness. The render class has no gamma field |
| Vignette position | Last, before masks. The render class has no vignette field |
| Slider strengths | Exposure, contrast and saturation are standard formulas. Highlights, shadows, whites, blacks, vibrance, fade, grain, halation and bloom strengths are my own, because the originals live in compiled shaders |
| Every tool added in this version | The reference is a feature list and five pictures, so the controls follow those and all the maths and strengths are this app's own |

## Presets

Thirty built-in presets in five groups, in `src/looks.rs`.

| Group | Looks | Origin |
| --- | --- | --- |
| Cinema | Deep Cine, Tealer, Deep Cine2, Brampton, Fuji D55 to D65, Kodak D55 to D65 | The original's |
| Deep Looks | Blades, Bind, Deep V, Greenery, G58 | The original's |
| Studio | Harbor Light, Neon Rain, Long Shadow, Blue Hour, Golden Road | This app's own |
| Film | Soft Daylight, Tungsten Night, Pastel Print, Silver Skip, Three Strip | This app's own |
| Mood | Quiet Morning, Dust Bowl, Cold Iron, Ember, Plain Silver | This app's own |

The original's fifteen names and their order are from `CinemaV2.json` and
`DeepLooksV2.json` in `DefaultLuts.bundle`, and their slider and colour grade values are
the numbers saved there. They should be replaced before this app is shared. The table
below covers only those fifteen.

| What | Rule | Status |
| --- | --- | --- |
| White colour grade | Saved as neutral, so left out | From preset JSON |
| Halation | `isEnabled` false or intensity 0 in all fifteen | From preset JSON |
| Grain | `grainIntensity` 0 and no grain stock chosen in all fifteen | From preset JSON |
| Bloom | Saved `bloom` block treated as off; values look like defaults | Estimated |
| Contrast with a print table | 1 + (saved - 0.9) x 0.6, because the original's log and print chain adds contrast this app's tables do not | Estimated |
| Bind contrast | Same rule; 0.3 taken literally looks washed out | Estimated |
| Greenery contrast | Saved 0 read as not set; literally it would be solid grey | Estimated |
| Which table | Fuji uses Cool Print, Kodak uses Warm Print, at the same D number | Estimated |

The Studio, Film and Mood looks were judged with `--contact`, a sheet of those fifteen
over a landscape, a grey ramp and three skin tones. That is a generated scene and flat
patches, not real photographs.

## Curves and Tetra screens

Both follow the reference screens supplied for this version. The behaviour is this
app's own, because a picture shows the controls but not the maths.

| What | Rule |
| --- | --- |
| Mode buttons | Six, each drawn from what it reads and what it changes |
| Graph backdrop | Rainbow for hue modes, black to white for Lum, grey to colour for Sat |
| Pale line on the graph | The picture before edits: hue weighted by saturation, or light, or saturation |
| Colour dots | Place a neutral point at that hue, or select one already within 2% |
| Number boxes | Input in degrees for hue and percent otherwise. Output in degrees for Hue vs Hue, a multiplier for saturation, an offset for luminance |
| Hue vs Hue range | 120 degrees from the bottom of the graph to the top |
| Tetra leans | Red, green and blue gain another channel. Cyan, magenta and yellow lose one. Grey never moves |
| Original Image | Hold to see the picture before any edits |
| Panel handle | Drags the panel taller or shorter, in the tall layout |

The reference's Input Hue reads 69.47 for a point about halfway along, so its unit is
not degrees. What that unit is could not be worked out from a picture.

## Masks and layers

Each layer holds its own edit and is blended over the picture through its
masks, after the base edit. A layer with no masks covers the whole photo, which
is the original's Image Edit Layer (wording at 0x101404b20).

| What | Rule | Where |
| --- | --- | --- |
| Subtract | base x (1 - mask) | `maskSubtract` text, 0x101410650 |
| Intersect | base x mask | `maskIntersect` text, 0x1014105c0 |
| Add | The larger of the two | Estimated |
| Layer actions | Add mask, reset edits, duplicate, delete, show or hide, invert, combine | Field names, 0x101390ab0 |
| Brush | Distance to the stroke's line; hard centre, smooth edge; softness is the faded share of the radius | `drawMaskStroke` text, 0x101410d00 |
| Eraser | existing - amount; painting keeps the larger | `drawMaskStroke` text, 0x101410d00 |
| HSL Qualifier | Hue centre, width, softness; saturation and luminance ranges | Field names, 0x101391d70 area |
| Pale colours | Ignored by hue selection (0.02 to 0.1 of saturation is estimated) | Release note, 0x101408370 |

Three requested tools have no match in the original's strings, so they are this
app's own design: Rectangle Mask (a turned box with a feathered edge), Curve
Mask (a closed smooth curve through placed points) and Shot Match (moves the
picture's light and colour averages and spread toward a reference photo).
Linear and Radial exist there, but their exact falloff lives in compiled shaders,
so the falloff here is estimated.

## Crop

The tools follow the original's crop library and Auto Crop note (0x101409164,
0x10141b5a0): drag handles, ratios, rotate, flip and reset. Mask positions are
kept on the untouched photo, as the original's mask crop filters suggest
(`maskCropFilter`). The 45 degree straighten reach, the ratio list and the Auto
Crop scoring are estimated.

## Color Space Transform

Log curves and primaries are the makers' published figures, tested against each
format's stated middle grey where one is published.

| Input | Curve | Gamut |
| --- | --- | --- |
| Apple Log | Apple Log Profile | Rec.2020 |
| ARRI LogC3 | Exposure index 800 | ARRI Wide Gamut 3 |
| Sony S-Log3 | Technical summary | S-Gamut3.Cine |
| DJI D-Log | DJI white paper | D-Gamut |
| Fujifilm F-Log2 | Data sheet | Rec.2020 |
| RED Log3G10 | RED white paper | REDWideGamutRGB |
| DaVinci Intermediate | Blackmagic white paper | DaVinci Wide Gamut |
| Insta360 I-Log | 10-bit I-Log White Paper, see below | Rec.2020 |
| HDR HLG | ITU-R BT.2100, with grey placed per BT.2408 | Rec.2020 |
| HDR PQ | SMPTE ST 2084, with grey at 26 nits per BT.2408 | Rec.2020 |

The Filmic tone map is a fitted filmic curve, not any camera maker's own transform.

I-Log: the constants come from an open-source tool (GT-Drones/Insta360LUT) that says it
reproduces Insta360's white paper. The paper itself was not read. They do land exactly on
the paper's quoted points: black at code 93, 18% grey at 432, 2200% at 1023. That tool's
author confirmed the curve on X5 ProRes footage only, and the paper doesn't list every
camera, so other models are unconfirmed.

## Film emulation

`src/film.rs` holds a small model of each stock (dye slopes, crosstalk, base colour)
and each print. **These are this app's own numbers, chosen to behave like the stock's
character; none are measured from the real film or from the original's tables.** Stock
and print names label the character being aimed for and are not a claim of accuracy.

## Video

ffmpeg and ffprobe do the reading and writing. The app looks in the folder named by
LOOKROOM_FFMPEG, then in an `ffmpeg` folder beside itself, then beside itself, then on
the PATH. If it finds nothing, it says how to install them. They are not bundled: the two
programs come to about 430 MB, and this build is GPL, which brings source-offer duties if
it is ever redistributed with the app.

Frames travel as 16 bits a channel in both directions.

| What | Rule |
| --- | --- |
| Export | H.264 and HEVC in MP4 or MOV, ProRes 422 HQ in MOV, up to 3840 on the long side |
| Sound and tags | Copied from the source |
| Clips with no colour tag | BT.709 from 720 lines up, BT.601 below |
| HDR clips | HLG and PQ are recognised from their tags and converted automatically. Choosing another input overrides it |
| Grading nothing | Measured 55 dB luma, 42 to 45 dB chroma against a tagged ProRes source |

HDR grey cards, converted: 38% HLG signal gives 47% on screen, 58% gives 72%, 75% gives 88%.

## Motion tracking

Up to 24 textured patches are matched against a held anchor frame, with a fresh anchor
every 16 frames. Measuring from an anchor rather than the previous frame is what stops
small errors adding up. A least-squares fit gives pan, tilt, zoom and rotate, leaving
out points that disagree. Tracking writes one key per frame; keys are also how manual
moves are stored, so the two mix freely.

If too few points match, the subject is treated as hidden: the mask carries on at its
last speed and keeps looking for the same patches for up to 48 frames. Tracking runs both
ways. Video only decodes forward, so backward tracking reads 48 frames at a time.

All clips below have an exactly known move, and positions are shares of the frame.

| Clip | True | Tracked |
| --- | --- | --- |
| Pan, frame 71 | 0.3891, 0.4014 | 0.3873, 0.4014 |
| Pan, 15 frames fully hidden | 0.3891, 0.4014 | 0.3877, 0.3997 |
| Pan with heavy grain | 0.3891, 0.4014 | 0.3827, 0.3982 (also 1.4° of false turn) |
| Fast pan with motion blur, frame 47 | 0.2188, 0.2500 | 0.2181, 0.2518 |
| Pan, tracked backward to frame 0 | 0.6109, 0.5986 | 0.6128, 0.5993 |
| Turning camera | 17.75° | 17.93° |
| Centre zoom, size | 0.2284 | 0.2292 (before the anchor change's smoothing) |

These clips are generated, not filmed: a rigid picture moved by a known amount. They
stress blur, grain and blocking, but not a subject that changes shape or turns away.
No real camera footage has been tested.

## Export

| Format | Depth | Metadata |
| --- | --- | --- |
| HEIC | 8 or 10-bit | EXIF kept, read and written |
| JPEG | 8-bit | EXIF kept, read and written |
| PNG | 8 or 16-bit | EXIF kept, read and written |
| TIFF | 8 or 16-bit | EXIF kept, read and written |

`src/tiff.rs`: the image library can't read or write TIFF tags. An Exif block is itself a
small TIFF whose offsets count from its own start, so the writer keeps it at the front of
the file and puts the picture and a new main directory after it.

`src/heic.rs`: ffmpeg can encode HEVC but has no HEIC container, so the file is built here.
Above 4096 pixels on the long side a picture is split into 512-pixel tiles, as phones do.
Eight-bit pictures are labelled Main Still Picture and ten-bit ones Main 10.

Opening: HEIC and AVIF go through ffmpeg, which assembles tiles and applies the stored
rotation. Files made here with both were read back upright, down to the pixels.

8 and 10-bit, single and tiled, were all read correctly by Windows' own decoder, ffmpeg and
ImageMagick. A camera tag survived JPEG to TIFF to JPEG to HEIC to TIFF. **None of this has
been opened on an iPhone or Mac, and no real iPhone file has been read.** Those need a
device. Display P3 photos are read as if sRGB, so they look slightly less saturated.

## Timeline

The layout follows a reference picture of a video editor supplied for this version. The
behaviour is this app's own. From 1100 points wide the window is the edit workspace;
narrower, it returns to the earlier layouts, and every grading tool is on the Adjustment page.

`src/timeline.rs` holds the project and every edit, with no drawing in it. Five tracks:
video, sound, two for effects, text and stickers, and music. Each clip keeps its own full
grade, so LUTs, curves, masks and tracking all apply per clip. The editor works on the clip
under the playhead. Undo covers the whole project.

Clips of other sizes are letterboxed into the project's picture, and clips at other rates are
converted to its rate. The first clip added sets both. A transition overlaps its two clips, so
the project gets shorter by its length. A clip reads its sound from its source until Detach
Audio moves that to an item of its own.

Effects, transitions, text and stickers are this app's own designs. Text is drawn through
SVG using Segoe UI, and stickers are Hugeicons.

## Playback

Measured on this machine, with frames timed as they reached the screen. All clips are
generated test footage, in a release build.

| Clip | Grade | Before | After |
| --- | --- | --- | --- |
| 1080p 24 | None | 184 of 192 shown, worst gap 242 ms | 240 of 240, worst 53 ms |
| 1080p 24 | Film look | 6.6 fps | 24.0 fps, played at 540 px |
| 4K 30 | None | 25.9 fps, worst gap 507 ms | 30.0 fps, worst 56 ms |
| 4K 30 | Film look | 6.2 fps | 30.1 fps, played at 400 px |
| 1080p then 4K, with music | Default look | Not possible | 384 of 384, worst 64 ms |

Each clip's colour is baked to a 33-point table, so a frame costs one lookup per pixel.
Frames are decoded ahead on one thread and finished ahead on another, and sound output is
the clock. Before playing, the real grade is timed on a small picture and the largest size
that fits the frame time is chosen, from 1280 down to 400 pixels. A heavy grade therefore plays
smooth but soft. The paused picture is always full quality.

Hardware decoding was tried and dropped: on this machine it was slower for 4K. Choosing the
size once matters, because starting a decoder on a 4K clip takes about 0.8 seconds.

Playback draws from the table, so a colour can differ slightly from the paused picture.
A test holds that difference under 3%.

## Speed

Timed with `--bench CLIP` and `--ui-bench OUT`, in a release build on this machine, with a
stress project of 500 clips and 2,000 items and a 1080p clip read at 720p. Pass a file to
`--ui-bench` to time that instead of the stress project.

| Work | Before | After |
| --- | --- | --- |
| Paused: step one frame forward | 544 ms | 26 ms |
| Paused: step one frame back, footage seen | 540 ms | 19 ms |
| Paused: go back to a frame just seen | 464 ms | 16 ms |
| Paused: first step back into unseen footage | Not timed | 1017 ms |
| Paused: jump ahead into unseen footage | Not timed | 538 ms |
| App work per frame, stress project | 9.77 ms, worst 15.28 | 1.00 ms, worst 2.69 |
| Copy the project | 553 us | 0.14 us |
| Compare, nothing changed | 727 us | 0.18 us |
| Which clip is at a frame | 2.78 us | 0.02 us |
| Which items are at a frame | 2.88 us | 0.18 us |
| Build all colour tables at startup | 276 ms | 59 ms |
| Table lookup, 1 million pixels, one thread | 42.6 ms | 24.3 ms |
| Frame to screen image | 6.20 ms | 2.40 ms |
| Re-render while dragging, same grade | 87 ms | 11 ms |

What each is built from:

- `Shared` in `src/shared.rs` is copy-on-write. Clips, items, media and each clip's grade sit
  behind one, so a copy is a reference count and compares start with the pointer. Undo steps
  share everything they didn't change. Per-frame code must read before it writes: any write
  takes a private copy.
- `Index` in `src/timeline.rs` is rebuilt when the project changes. Clip starts are searched by
  bisection. Items sit sorted by start with the furthest end so far beside them, which lets a
  search stop once nothing further back can overlap.
- `Lru` in `src/shared.rs` holds decoded frames for the paused picture, up to 320 MB per clip,
  for the clip on screen and the last two. The feed reads ahead while idle and starts 16 frames
  early when going backward.
- The first look at footage still has to start ffmpeg, so the two unseen rows above stay slow.
  A keyframe index would be the next step there.
- `Lut3D::sample` is tetrahedral: four corners where trilinear read eight.
- The timeline draws only what's visible. Anything under 14 points wide is a plain bar and can
  only be grabbed near the pointer.
- While a slider or key is held the view uses a small baked table, then renders exactly once on
  release. A click that changes nothing renders nothing. Left paused, the window measured 1.4%
  of one core.

## Keyboard

`src/ui/keys.rs` holds one table that drives both the keys and the help sheet (F1, ?, or
Settings), so they can't disagree. A test checks that no two entries share a key. Nothing fires
while a text box has focus or a sheet is open. Holding a key repeats only moves, zoom and undo.
Full screen (F, the button by the clock, or a double-click on the picture) shows the picture
and its play controls alone, filling the screen. It used to make the window full screen with
the editor laid out as before, so the picture stayed small. Esc or F leaves it.
The Motion group covers keyframes (Ctrl K, Ctrl Shift K, `[`, `]`) and clip speed (R, Ctrl Up,
Ctrl Down).

## Moving text and stickers

The animation styles follow Jitter's ideas: an In, an Out and a Loop, each run across the
whole text, its lines, words or letters, with a stagger between pieces, an order (forward,
reverse or random) and an easing curve. The animations themselves are this app's own, in
`src/textanim.rs`. Nothing there draws; it says where each piece goes, and `src/overlay.rs`
draws each piece once and moves the drawing. Stickers go through the same code as a single
piece, so they get the same animations and keyframes.

The Text and Sticker pages show every animation as a tile. A tile shows its animation part of
the way through, and plays it on a loop while the pointer is over it.

Keyframes set position, scale, rotation and opacity together, each with the curve to the next
one. With no keyframes, the sliders set plain values. With keyframes, a change at the playhead
sets a keyframe there. They show as diamonds on the selected item: click one to go to it, or drag
it to move it in time. Splitting an item or trimming its start keeps every keyframe at the same
moment of the picture.

Fonts are the computer's own, plus any added with Add a Font File, which are copied into
`%APPDATA%\Lookroom\Fonts`. The list draws each name in its own font, at most three new ones a
frame, and keeps the last 160 drawn. A project names its font. On a computer without that
font, the text falls back to Segoe UI.

## Clip speed

From 0.1× to 8×, by the Speed button under the timeline, R, or Ctrl Up and Ctrl Down. A clip
keeps the same stretch of its source, so faster makes it shorter and everything after it moves
up. Pictures are retimed by ffmpeg (`setpts`, then the project's rate) and sound by `atempo`,
which keeps the pitch. Trimming, splitting, the paused picture, playback and export all read
the source through `Clip::source_at`, so they agree. A 2× read of 7 seconds of a 24 fps test clip
gave exactly 84 frames.

A project's rate comes from its first video. A clip added before that rate is known used to keep
the old rate's length: an 8 second 24 fps clip came in as 10 seconds. `Project::rate_to` now
converts every clip, item and keyframe to the new rate.

## Plugin packs

A pack is one JSON file in `%APPDATA%\Lookroom\Plugins` or a `plugins` folder beside the app.
Settings lists what is loaded and why anything wasn't, and can install a pack, reload them all, or
open the folder. Packs are data only. Nothing in one runs, numbers are kept to sensible ranges,
and a file over 2 MB is refused. `plugins/lookroom-extras.json` is built into the app and doubles
as the example:

```json
{
  "name": "My Pack", "author": "Me", "version": "1.0", "description": "Shown on hover.",
  "text_animations": [
    { "name": "Rise", "unit": "Letter", "order": "Forward", "seconds": 0.7, "delay": 0.04,
      "ease": "Back", "from": { "opacity": 0, "dy": 0.9, "rotate": -40, "scale": 1, "dx": 0, "blur": 0 } }
  ],
  "looks": [ { "name": "Night", "grade": { "temp": -900, "contrast": 1.15 } } ]
}
```

`from` is where each piece starts. It eases from there to rest, and offsets are in text heights.
Units are Whole, Line, Word or Letter. Orders are Forward, Reverse or Random. Curves are Linear,
In, Out, InOut, Back, Expo, Circ, Elastic, Bounce or Spring. A look's grade takes any of the
grade's own fields, as saved in a project. Looks appear under Plugins among the presets. A text
keeps its plugin animation's numbers, so it still plays after the pack is removed.

## Stickers, sounds and memes

Each library was picked for a licence that allows shipping it inside the app. Settings lists
every credit and licence text.

- **Stickers:** Fluent Emoji Flat by Microsoft, MIT, taken from `@iconify-json/fluent-emoji-flat`
  1.2.6. The 1,586 emoji without skin-tone variants are in `assets/stickers` as one SVG body per
  line, compiled into the app and drawn by resvg at whatever size the picture needs. The page
  has groups and search, and draws at most 12 new tiles a frame.
- **Sounds:** three Kenney packs, CC0, from OpenGameArt (kenney.nl blocks scripted downloads):
  Interface (100), Digital (62) and Foley (51). Fourteen editing staples (whoosh, swoosh, riser,
  boom, hit, pop, ding, chime, censor beep, tick, heartbeat, shutter, glitch, bass drop) are
  synthesised by `src/assets.rs`, so they need no licence. All 227 sounds are compiled in and
  written to `%APPDATA%\Lookroom\Sounds` when first heard or added. The add button puts one on
  the music track at the playhead.
- **Memes:** templates come from Imgflip's public list of its 100 most used (no key needed).
  Nothing is bundled. The list and each picture are fetched with the system's curl when the Memes
  tab asks, only from `i.imgflip.com`, and kept in `%APPDATA%\Lookroom\Memes`. A template becomes
  a photo clip with top and bottom captions in Impact, white with a black edge. The edge is a new
  Outline setting that any text can use. The pictures belong to their owners.

## Effects, transitions and previews

There are 23 effects and 124 transitions, drawn on the processor in `src/overlay.rs` and
`src/transitions.rs`. The transition catalogue includes Classic, Masks, Wipes, Motion,
Distortion, Texture and Light categories, searchable by name or category. A test
checks every transition on every pixel: exactly the outgoing picture at its start, and exactly
the incoming one at its end. Writing that test turned up a one-pixel line of the old picture
left at the end of Wipe Left and Wipe Up, now fixed.

The procedural transitions are original presets inspired by common compositing techniques,
not imported After Effects plugins. Shape masks include circles, ellipses, diamonds, boxes,
stars, hearts, hexagons, slits and crosses. Luma/chroma masks follow the outgoing image's
brightness/saturation; ink, cloud, brush, grain and paper mattes use deterministic textures.
They are preset mattes, not subject tracking or hand-drawn rotoscoping. Directional moves,
whip blur, zoom/spin, ripple, swirl, RGB split, glitches, light leaks and bloom share the same
renderer in preview and export. Mask edges are antialiased, with feathered presets; shape
geometry accounts for the frame's aspect ratio. Existing saved transition identifiers stay
compatible. Tests cover all presets, serialization, distinct output, portrait/landscape,
determinism, matte progression and playback/export agreement.

Interface type is capped at the medium size, 14 logical points, in the shared font helpers
and egui text styles. Video text and template/animation artwork retain their content sizing.

Previews:
- Each tile shows its effect or transition at 192 × 108, on the frame being edited (and, for
  transitions, the sample scene as the second picture). At rest a tile shows the middle; while
  the pointer is over it, it plays from start to end on a loop.
- The transition grid scrolls within a bounded panel and renders visible tiles only. Search,
  category chips, the active transition and duration controls remain outside that grid.
- **Preview on the Picture**, or **P**, plays the picked effect's stretch, or the transition
  nearest the playhead with half a second either side, then stops there.

## Resizing

- **Panels:** drag the gap between the bin and the picture, or between the picture and the
  timeline. Double-click a gap to put it back.
- **Tracks:** drag the line under the video track's controls to make every track taller or
  shorter, from 60% to 250%. Double-click it to put it back.
- **Remembered:** sizes are kept in `settings.json`, so the window opens the way it was left.
  The picture never gets narrower than 320 points or the bin than 260.

## Design layers (Figma)

- **Bringing a design in:** in Figma, pick a frame and choose Copy as SVG, then press Ctrl+V
  in the app. Import a Figma or SVG File on the Layers tab does the same from a saved file.
- **Layers:** each top-level layer becomes a Graphic item of its own, named as in Figma, at
  its place in the frame. If the frame arrives wrapped in one plain group, the group is opened.
  A group that moves its children stays one layer, so nothing shifts.
- **Drawing:** each layer is drawn alone by resvg with `render_node`, and drawn again only
  when its size on the picture changes. Designs are kept in `%APPDATA%\Lookroom\Graphics`
  under a name taken from their text.
- **Animating:** a layer gets the same In, Out, Loop and keyframes as text. Animate Every Layer
  gives the whole design one In, each layer starting a stagger after the one below, as Jitter
  does. Their ends stay put.
- **The Layers tab:** lists everything laid over the picture at the playhead, top first: text,
  stickers, videos and design layers. Each can be shown or hidden, brought forward or sent
  back. Later in the project's list draws on top, so the order on screen matches the list.

## Video memes

There is no trending feed:
- Tenor's API shut down in June 2026.
- GIPHY's terms forbid storing copies of its media, which putting a clip in a video does.
- KLIPY doesn't publish its rules on that.

The Clips tab is a library of the person's own clips instead, kept in
`%APPDATA%\Lookroom\Video Memes`, with folders as groups. Picking one lays it over the picture
as an overlay with its own sound, size, position, animations and keyframes.

Green screens:
- **Detection:** a clip counts as green screen when at least 60% of its first frame's border
  is one green.
- **Keying:** that colour is removed by its hue relative to brightness, so shadows on the
  screen go too. The screen's spill is pulled back out of the edges.
- **Placement:** a keyed clip fills the frame. Any other clip sits in a corner, picture in
  picture.

Overlays are decoded beside the main clips, so playback, export and the paused picture all
show them. A paused frame with an overlay goes through the full compositor, which starts a
decoder, so it takes a moment longer than a plain frame.

## Linux and macOS

The code builds on all three systems. What differs lives in `src/platform.rs`:

| | Windows | macOS | Linux |
| --- | --- | --- | --- |
| The app's files | `%APPDATA%\Lookroom` | `~/Library/Application Support/Lookroom` | `$XDG_DATA_HOME/lookroom` (`~/.local/share/lookroom`) |
| Interface font | Segoe UI, read straight from its files | SF Pro or Helvetica Neue, found among installed fonts | Inter, Noto Sans, Cantarell, Ubuntu or DejaVu Sans, the first installed |
| Open Folder | Explorer | `open` | `xdg-open` |
| ffmpeg | `winget install Gyan.FFmpeg` | `brew install ffmpeg` | the distribution's package |

The differences are runtime checks rather than conditional compilation, so a Windows build
compiles every system's code path. Only the console-window flag on Windows is compiled
separately.

Packages:
- **Linux:** `packaging/build-linux.sh` makes `dist/lookroom-linux-<arch>.tar.gz`. It holds
  the program, a 512-pixel icon, a desktop entry and `install.sh`, which installs for the
  current user only (`~/.local/bin`, the menu and the icon).
- **macOS:** `packaging/build-macos.sh` makes `dist/Lookroom.app`, one universal program for
  Apple Silicon and Intel, with its icon set. It signs it for the building Mac only and zips
  it. Anyone else opens it the first time with right-click, Open, until it is signed with a
  Developer ID and notarised.
- **Icon:** the tiger logo. `assets/icon.png` (1024 pixels) is the source, its black corners
  made transparent along the artwork's own rounded edge. `lookroom --icon OUT SIZE` writes it at
  any size as a PNG, which is how both scripts get it, and the window uses it on every system.
  On Windows, `build.rs` also puts `assets/icon.ico` into the program itself, for Explorer and
  the taskbar.

`.github/workflows/build.yml` runs the tests and builds all three on GitHub's machines:
Ubuntu 22.04, macOS 14 with both Mac targets, and Windows. It uploads the packages, and a
tag such as `v0.2.0` attaches them to a release.

This machine has neither WSL nor Docker, and macOS programs can only be built on a Mac. So
only the Windows build has been run here; the Linux and macOS ones run first in that
workflow.

## The look

Reviewed with the Hallmark design skill (`~/.claude/skills/hallmark`), using its rules for
app screens, and with two reference images: a violet slider set with value bubbles, and a
dark set of tactile controls. Hallmark's rules for web pages (CSS tokens, hero, fold) don't
apply to a native app.

What changed:
- **One accent, violet (`#7B61FF`):** it marks the page's main action, a value being set and
  state. It replaced a bright blue that filled tabs, tools and rows all at once.
- **Selection is quieter.** A chosen tab or segment is lifted in a raised grey, and a chosen
  row or chip is a soft wash of the accent with a thin edge.
- **Sliders follow the reference:** a 4-pixel track, a violet fill from the neutral point,
  and a white knob ringed in violet. While a knob is pointed at or dragged, its value sits in
  a bubble above it, drawn above every panel so nothing cuts it off. Colour-band sliders
  keep their bands.
- **Focus rings:** every custom control shows one when the keyboard moves to it, drawn at
  once. They show after a Tab press and hide again after a pointer press.
- **Errors in red:** a failed download, an unreadable clip or a pack that didn't load reads
  in `ERROR` red, not hint grey.
- **Play is a white disc,** the one transport control found first.
- **Named colours:** the grey used for secondary text everywhere is now the `TEXT_2` token.

Not changed, because a restyle keeps the layout:
- the panel arrangement and the timeline's item colours;
- the multicoloured icons in the Adjustment tool row.

## Auto Captions

The plugin that comes with the app. It is on the Text page, under Captions, and has a row in
Settings with the other plugins. `src/captions.rs` does the listening and
`src/ui/captions.rs` the page.

How it works:
- The timeline's sound is mixed the way playback mixes it, by `mix_graph`, then turned down
  to 16 kHz mono and sent through ffmpeg's `whisper` filter. That filter runs whisper.cpp
  with OpenAI's Whisper models. It writes SubRip, and `parse_srt` reads it back, leaving out
  lines Whisper marks as not speech, such as `[Music]`.
- It hears the whole timeline or only the clip under the playhead. Each caption is a text
  item on the second effects track. It lasts until the next caption starts, so there are no
  blank moments between them. With Replace on, captions from an earlier pass in the same span
  are removed first.
- Captions are marked as one group (`Motion.group = "captions"`). That lets Restyle All
  change their look in one step without losing a font or keyframes set by hand, and lets
  Remove All take them away.
- Five styles: Clean, Bold, Pop, Rise and Typewriter. Each is a size, a weight, an outline
  and an entrance, set 86% of the way down the picture.
- The transcript below the settings lists every caption. Its text can be edited there, and
  clicking a time moves the playhead to it.

Models are downloaded once, with the system's curl, from whisper.cpp's page on Hugging Face.
They go to `Models` in the app's data folder (`%APPDATA%\Lookroom\Models` on Windows):
- Tiny, 75 MB;
- Base, 142 MB, the default;
- Small, 466 MB.

Both the models and whisper.cpp are MIT licensed. Nothing but the model download leaves the
computer.

The ffmpeg in use must include the whisper filter; ffmpeg 8 and later builds with it. If it
is missing, the page says so and does not offer Generate.

Measured here on Base: a 13.7-second spoken clip took 13.3 seconds and gave four correct
lines. `lookroom --captions FILE` prints a file's captions as SubRip, using the first model
already downloaded.

## Timeline editing

- **Snapping:** a moved or trimmed clip or item snaps to the playhead, the start, and the
  edges of every other clip and item. It snaps within 9 screen points, so the reach in frames
  follows the zoom. A violet line marks the edge it snapped to.
  - N or the magnet button turns snapping off and on.
  - Holding Alt turns it off for one drag.
- **Readouts while dragging:** a bubble shows the new start and length, or the trim. Moving a
  clip shows a marker where it will land.
- **Zoom:** the corner above the track names has zoom out, a zoom bar that works in steps of
  ratio, zoom in, and Fit. The - and = keys, and Ctrl with the scroll wheel, zoom as before.
- **The ruler** picks its step from the zoom, down to single frames, with minor ticks between
  labels.
- **Clicking an empty lane** moves the playhead there and clears the selection. Dragging on
  it scrubs.
- **Empty tracks** show their names faintly, and an empty project says how to start.
- **Following the playhead:** while playing, or while the arrow keys move the playhead, the
  view scrolls to keep it on screen.

Checked in the running app by dragging a caption toward the next one:
- with snapping on, it ended exactly where the next one starts (frames 96 to 121);
- with N pressed first, the same drag ended a frame short (95 to 120).

## Editing several at once

How to select:
- Ctrl or Shift and a click on the timeline adds an item to the selection, or takes it away.
  The same works in the Layers list. A plain click selects one again.
- Ctrl A, or Select All at the right of the bar below the timeline, selects every item. The
  panel keeps the item it showed; with none, it opens Text on the text at the playhead.
- Auto Captions selects the captions it writes. Clicking a caption opens the Text tab; while
  it is the only one selected, Edit All Captions there selects every caption. The same
  button is on the Captions tab.
- Text and Captions keep their own scroll positions, so Text always opens at its top.
- Importing a design selects all of its layers.

The panel shows one item of the selection, under a banner saying how many are being edited.
Just This drops the rest. `ui/many.rs` holds the selection; the group only lasts while the
item shown is part of it, so picking anything else ends it.

After the panel draws each frame, `timeline::spread` copies what changed on the shown item
to the others, one setting at a time:
- copied as they are: font, weight, italic, spacing, outline, colour, opacity, In, Out,
  Loop, an effect's kind and amount, and volume;
- moved by the same amount: position and rotation;
- multiplied by the same ratio: size and scale. So 64-point captions set to 96 come out
  96, and a design's layers keep their layout as they grow;
- never copied: what each one says, its keyframes and its name.

Different kinds share what they have in common: a sticker in a group of captions takes the
new animation but has no colour to take. Dragging one selected item moves them all by the
same number of frames, and Delete removes them all.

Checked in the running app on four generated captions, all selected:
- the yellow swatch and the size slider changed all four (64 to 113 points), each keeping
  its own words;
- choosing Georgia gave all four Georgia, keeping their weight and Pop animation;
- dragging one moved all four 22 frames, keeping the gaps;
- Ctrl A then Delete removed them.

## Brand kits

Each company keeps a kit on the Brand page. `brand.rs` stores the kits; `ui/brand.rs` is the
page.

A kit holds:
- a name;
- a logo;
- up to 8 colours, and a title colour;
- fonts for titles and body text;
- a caption look: font, size, colour, weight, outline and one of the Auto Captions styles;
- a watermark: on or off, corner, size and opacity;
- a brand voice, which the assistant writes in.

**Storage.** Each kit is one JSON file in `%APPDATA%\Lookroom\Brands`, with the logo inside
it. Share Kit saves that file as `.lookbrand`. Import Kit reads one and gives it a new id,
so it never replaces a kit already there.

**Logos.** A PNG, JPEG, WebP or SVG logo is wrapped in an SVG of its own size, as an
embedded picture. So every logo, however many shapes it has, draws as one design layer.
This needed resvg's `raster-images` feature, which also lets Figma designs with pictures
in them draw.

**Use on This Project.** It records the kit on the project (`Project.brand`), then:
- puts the logo in its corner as a design layer the length of the timeline, sized by
  height so wide and tall logos weigh the same;
- gives every caption the kit's caption look;
- gives every other text the kit's title font.

Applying it again replaces the logo instead of adding a second. From then on, new text
starts in the kit's font and title colour, new captions take its caption look, and the
Text page shows the kit's colours under the usual ones. Stop Using clears the kit from the
project and changes nothing already made.

Checked in the app with a test kit, an orange round logo and Georgia: the logo appeared in
the top right, and a new title came out in Georgia in the kit's orange.

## Assistant

The Assistant page is a chat with Claude (`claude-opus-5-5`) that changes the open
project. `assistant.rs` sends the requests; `ui/assistant.rs` holds the page and the tools.

**Key.** It uses the person's own Anthropic API key, typed on the page and kept in
`settings.json`, or the `ANTHROPIC_API_KEY` variable. The key reaches curl on its standard
input, never on a command line.

**Requests.** Rust has no official Anthropic library, so requests go over HTTP through the
system's curl, as the caption models' downloads do. Each request:
- uses API version `2026-10-21` and effort `medium`;
- turns on the server's fallback model for declined requests (`fallbacks: "default"`, beta
  `server-side-fallback-2026-07-01`);
- turns on automatic prompt caching.

The system prompt and the tools never change during a chat, so the cached part stays valid.

**Conversation.** The history is only ever added to, and each reply goes back exactly as it
came, thinking blocks included. After a stopped run, the person's next words join the last
turn, so turns still alternate. A message may take at most 24 rounds of tools. A declined
request or an answer cut short is said plainly.

**Tools:**
- reading: `get_project` (clips, items, selection, playhead and brand kit, as JSON) and
  `find_fonts`;
- text and items: `add_text`, `edit_items`, `delete_items`, `select_items`;
- picture: `add_effect`, `set_look`, `adjust_colour`, `add_transition`, `set_aspect_ratio`;
- the rest: `apply_brand`, `generate_captions`, `seek`.

Names of looks, effects, transitions and animations come from the app's own lists, so the
model can't invent one. A wrong font or colour comes back to it as an error, never a guess.
Its changes are ordinary project edits, so the undo history records them and Ctrl+Z undoes
each one.

**Checked.** With a deliberately invalid key, a request went out and Anthropic's
authentication error came back as "The API key wasn't accepted". That shows the key, curl
and reply handling work. A full conversation with a real key has not been run here, and
unit tests cover the tools' changes and the request's shape.

## KLIPY video memes

Sticker → Clips opens on KLIPY. My Clips, beside it, is the person's own library as before.
`klipy.rs` talks to KLIPY's API; `ui/klipy.rs` is the panel.

**The panel:**
- trending clips first, then whatever is typed in "Search KLIPY" (Enter or Search);
- 24 a page, with Load More;
- each tile shows the first frame of the clip's 320-pixel preview, and its title;
- "Powered by KLIPY" sits under the search, as is usual for this kind of API.

KLIPY's attribution page only renders in a browser, so check its exact rules at
docs.klipy.com/attribution before release.

**Adding a clip.** A click fetches the clip's MP4 in the background and lays it over the
picture at the playhead, like a clip from My Clips. Its first frame is checked for a green
screen, which is then keyed out; other clips go in a corner, picture in picture. The size,
key and volume settings show under the results.

**Requests and files:**
- `GET https://api.klipy.com/api/v1/{key}/clips/trending` and `/clips/search?q=`, with
  `page`, `per_page` and a `customer_id`;
- the customer id is anonymous, made once per computer and kept in settings;
- anything that isn't a clip (ads) is left out;
- files are only ever fetched from `static.klipy.com`;
- previews and clips are kept in `%APPDATA%\Lookroom\KLIPY`, so a clip used once works
  offline.

**The key.** It is kept in `settings.json` (`klipy_key`) or the `KLIPY_API_KEY` variable,
never in the code, since the repository is public. On this computer it is in both: the
user environment variable holds it, so it survives settings being reset. It sits in the request's address, so
the address reaches curl on standard input, not the command line. Settings shows whether a
key is saved and can remove it.

**Checked in the app.** Trending loaded with previews. A search for "surprised" found
matching clips, and clicking one fetched it (173 KB, 3.3 seconds) and put it over the
video at the playhead.

## Cuts

Transitions has a second tab, Cuts (`ui/cuts.rs`).

**On the cut nearest the playhead:**
- Hard Cut: no transition, and the sound cuts with the picture.
- Smash Cut: a 4-frame White Flash and 9 frames of Camera Shake on the cut.
- J Cut: the next clip's sound starts up to 2 seconds early (`Clip.lead`).
- L Cut: this clip's sound runs on up to 2 seconds (`Clip.tail`).

J and L cuts are real split edits. `Project::sound_windows` starts or ends each clip's
sound early or late, moves the neighbouring clip's sound out of the way, and never reaches
past either source's ends. So a clip that already uses its whole source has nothing to
lead or trail with. Splitting a clip keeps the J cut on the first piece and the L cut on
the second.

**At the playhead:**
- Jump Cut: splits the shot and drops the next 0.1 to 3 seconds of it. With Punch In on, the
  new piece is enlarged to 1.15 times; a run of jump cuts goes in and out by turns rather
  than ever closer.
- Punch In: enlarges the clip under the playhead from its centre (`Clip.zoom`).
- Cutaway (B-roll): lays another video over the main shot, full frame (enlarged to cover the
  width when its shape differs) and silent, so the main shot's sound carries on.

Cutting on action, match cuts and cross-cutting depend on what is in the shots, so they are
left to the person, or to the assistant with the tools below.

**Checked in the app:**
- Smash Cut put the flash and shake on the cut.
- Jump Cut at 0:03 dropped half a second; the next frame's burned-in timecode reads 3.533.
  The punch-in shows in the paused picture too.
- A 2-second cutaway at 0:02 showed the other video full frame, silent.

## Speed ramps

The Speed panel has ramps:
- **Presets:** Montage, Hero, Bullet, Speed Up, Slow Down and Flash In.
- **The curve:** speed on a scale where each doubling is the same height, from 0.1 to 8
  times. Drag a point to change it, double-click to add one, right-click to remove one.

How it works:
- A ramp is points of (how far through the clip, speed), with straight lines between them.
- `Clip::source_at` adds up the speed exactly (`ramp_area`) to find the source time for
  each frame; `offset_of` turns that back the other way.
- Choosing a ramp keeps the same footage, so the clip's length becomes the footage divided
  by the ramp's average speed. Dragging keeps the length, within the footage there is.
- The source is read at its own speed and the composer picks the frame the curve calls
  for, repeating frames in slow parts and skipping ahead in fast ones.
- A ramped clip plays without its own sound, which would warble; music under a ramp is
  the usual way.
- Splitting a ramped clip gives each piece a steady speed, its part's average.

Checked: Montage made a 240-frame clip 92 frames, its 2.6 times average. One second in, the
picture showed the source at 3.0 seconds, as 3 times says. Dragging the middle point from
0.35 to 1.45 times reshaped the curve.

## Text styles and templates

**Style section.** The Text page has a Style section (`ui/templates.rs`):
- **Gradient:** the letters blend down each line into a second colour.
- **Highlighted words:** words between `*stars*` take the highlight colour and are drawn at
  least extra bold (800). The stars never show.
- **Behind the text:** Box, Pill, Marker (a rough brush stroke with bristle streaks),
  Underline (hand-drawn) or Frame (an outlined box), in a colour of its own.

**How it's drawn.** The gradient and highlights go into each piece's SVG, so letter-by-letter
animation still works. The shape is drawn once at the text's size, kept until the text
changes, and moves with the text's animation. Its wobble comes from the item's id, so it
never shimmers. All of this lives on `Motion`, so selected items share it when edited
together.

**Templates.** The Text page also has a Templates tab, with title layouts made of ordinary
text items:
- Marker Title;
- Callout;
- Two-Tone Headline;
- Big Word;
- Pill Tags;
- Lower Third;
- Stacked Statement;
- Underlined Note.

Each lands at the playhead with its parts timed after one another and selected together,
in the brand kit's title font when there is one. All eight were placed in the app and
drawn as designed.

**Backgrounds.** The same tab adds colour fields as clips at the playhead, in six palettes:
- a still diagonal blend, saved as PNG;
- or a 10-second moving glow from ffmpeg's `gradients` source, softened with `gblur`.

They are kept in `%APPDATA%\Lookroom\Backgrounds`.

**Bug found while testing.** egui's colour buttons write their colour back every frame
after a round trip through another colour space. With items selected together, that
counted as an edit and copied one item's colours onto the rest. `kit::colour_button` now
writes back only when a colour is picked; the Brand page uses it too.

## Flicker effects

Three effects join Effects:
- **Light Flicker:** uneven brightness, with now and then a deep dip.
- **Shutter Flicker:** black frames at uneven gaps.
- **White Flash:** white at once, fading out; the smash cut uses it.

Each takes its pattern from the frame number, so it plays the same every time.

## Assistant models and editing tools

The Assistant page has a model choice, kept in settings:
- Opus 5.5, the default;
- Sonnet 5.5;
- Haiku 5.5;
- Fable 5.1.

Haiku has no fallback model for declined requests, so its requests go without
`fallbacks` and without that beta header.

**What it reads.** `get_project` now lists the media bin and, for each clip, its source
start, ramp, punch-in and J and L cuts.

**New tools:**
- building the A-roll: `add_clip` (a stretch of a bin file, placed at the end, the start,
  the playhead or after a clip), `split` and `remove_range` (cut a stretch and close the
  gap);
- cuts: `set_cut` (hard, J, L, smash) and `jump_cut`;
- B-roll: `add_cutaway`;
- speed and framing: `set_speed` (a steady speed or a named ramp) and `set_punch_in`;
- titles: `add_template` (with the words to use).

`add_text` and `edit_items` take gradient, highlight and the shape behind the text. The
system prompt explains A-roll and B-roll, and asks the assistant to say when a shot choice
is a guess, since it can't see the footage.

## Crashes on big projects

**The main cause: too many decoders at once.** Opening a project started a film strip
for every source file at once, each its own ffmpeg process decoding the whole file.
The NBDF projects have 159 source files, mostly 4K camera MOVs, so about 159 ffmpegs ran
together at 100 MB or more each and used up all 16 GB. Lookroom itself held only 427 MB
when it failed to get 11 MB more ("memory allocation of 11059200 bytes failed"), and the
watching script ran out of memory at the same moment. That is why it crashed when looking
at a saved project.

Film strips now go through a queue served by two workers (`STRIP_WORKERS`), each ffmpeg
limited to two threads. They arrive one after another instead of all at once.

**Also: caches that grew without limit.** The NBDF video project (4K, 242 clips, 678 items)
also printed "memory allocation of 2764800 bytes failed", and Windows had flagged the app
for runaway memory. Three things inside Lookroom grew without limit:
- **Drawn text, sticker, shape and design pictures,** kept for every item ever shown. At 4K
  a caption takes about 10 MB as stored (four floats a pixel), so 523 texts reach 5 GB in an
  export, and the preview kept its own set for the whole session.
- **Export's staged colour tables,** 6.6 MB a clip, kept for every clip passed: 1.6 GB over
  242 clips. Playback's tables too.
- **Each clip's queue of decoded frames:** four frames of about 100 MB each at 4K.

**The fix:**
- Drawn items are now let go once they are more than 2 seconds from the frame being made
  (`Laid::forget_far`); coming back draws them again, which takes milliseconds.
- Colour tables are kept only for the clips on screen and the next one.
- Decode queues are sized to 160 MB, which is one frame at 4K and four at 1080p.

**Checked.** Exporting the 4K project now holds at 500 to 600 MB, flat; it used to climb
until it failed. The Reel project sits at about 385 MB open. A test checks the text cache
stays small over a long timeline.

**Safety nets** (`ui/safety.rs`):
- Any panic writes a report, with a backtrace, to `%APPDATA%\Lookroom\Crashes`.
- While the project changes, it is saved in the background every minute to
  `%APPDATA%\Lookroom\Autosave`, written aside and then renamed, so a crash mid-write leaves
  the last good save.
- Each running window leaves a marker named for its process. At start, a marker whose
  process is gone means that window crashed, and Lookroom offers to restore its autosave.
  A second open window is never mistaken for a crash.
- Running out of memory ends a program without a report, so the autosave is the cover for
  that.
- Scripted test runs skip all of this.

## Picking clips and copying adjustments

**Picking:**
- Ctrl and a click on a clip adds it to the picked clips or takes it away.
- Shift and a click picks the run from the last picked clip.
- A plain click picks one, and moves the playhead.
- Picked clips have a yellow edge; the main one keeps its trim handles.
- Delete removes them all.

**The right-click menu of a clip** (`ui/clipsel.rs`):
- **Adjustments:**
  - Copy Adjustments takes the clip's whole grade.
  - Paste Adjustments puts it on every picked clip, and Paste to Every Clip on all of them.
    Each keeps its own crop and frame shape, which belong to its picture.
  - Reset Adjustments clears the grade, keeping the framing.
- **Picking:** Select Every Clip, and Select Clips From This File.
- **Sound:** Mute or Unmute, and Clean Up Sound, on every picked clip.
- **Delete.**

**Sound of several clips.** The Audio page opens with Clip Sound, or "Sound of N Clips"
when several are picked. Volume, Mute and Clean Up Noise there change every picked clip at
once.

## Muting and volume of clips

What was wrong:
- **Playback.** Playback built its sound mix when it started, so muting a clip or changing
  its volume while playing wasn't heard until the next play.
- **Two mutes that disagreed.** The toolbar's mute acted on the clicked clip, and silently
  did nothing without one; the M key acted on the clip being edited. The button's icon
  never showed the state.
- **The Original Sound bar** under each clip could only be looked at, and always showed
  the sound-on icon.

What it does now:
- **Playback** keeps the sound it started with (`Run::sound_at_start`) and restarts from
  the same moment when the project's sound changes, as a picture change already did.
- **One mute** (`toggle_clip_mute`) serves the toolbar button and M: it mutes the picked
  clips, or the clip being edited when none is picked, and unmutes them if they're all
  muted. The button shows a crossed-out speaker when they are.
- **The Original Sound bar:**
  - its speaker mutes and unmutes;
  - a click picks the clip, with Ctrl and Shift as on the clip;
  - dragging up or down sets the volume of every picked clip, up to 200%, with the value
    in a bubble and a line while pointed at;
  - a double-click puts it back to 100%;
  - a right-click opens the clip's menu;
  - its label says "muted", or the volume when it isn't 100%.

Checked in the app on two clips:
- clicking the second clip's speaker muted only that clip;
- dragging the first clip's bar raised only its volume, to 163%;
- Ctrl-click on both, then M, muted both;
- muting a clip during playback restarted the sound at once.

**Film strips.** Each strip now decodes only the key frames, in one pass, and picks its
pictures evenly from them. For a 16-second 4K camera clip that takes 5.3 seconds instead
of 37.7, so a big project's strips arrive in minutes, not tens of minutes.

## Clean up noise

Each clip, and each sound item, has a noise clean-up strength (`denoise`, 0 to 1). It
survives Detach Audio. In the mix, a part with clean-up gets a high-pass at 60 to 90 Hz,
for rumble, then ffmpeg's FFT noise reducer (`afftdn`). The reducer learns the steady noise
as it goes and takes it down by 6 to 30 dB, as the strength rises. It suits wind, fans, hum
and room hiss; very strong settings can make voices sound thin. Playback and export share
the mix, so both hear it.

## How long an export takes

**Before.** The Export sheet shows "Takes about … on this computer". `player::estimate`
makes a few real frames at four points across the timeline, at the export's size, not
counting the first at each, which starts the decoders. From the median it works out the
whole: times the number of frames, plus a share for writing the file (more on the
processor than on a graphics chip) and a moment for each clip's decoder to start. It
runs in the background and is worked out again when the choices change.

**During.** The progress shows the share done and the time left at the pace so far.

**Checked.** For the 4K NBDF video project the estimate said about 5 h 50 min; its real
pace over the first 350 frames gives 5 h 41 min. `lookroom --estimate PROJECT [SIZE]`
prints the estimate.

## Video layers

**On the picture** (`ui/onpicture.rs`). The picked layer (a video over the picture, a
text, a sticker or a design layer) shows an outline and a corner handle on the picture:
- drag inside it to move it;
- drag the handle to resize it.

The outline's size comes from the layer itself: a video's height and shape, a sticker's
size, a design layer's bounds, or roughly a text's letters. A keyframed layer gets a
keyframe at the playhead, as with the sliders. Text and stickers without keyframes grow by
their own size, so letters stay sharp.

**On the timeline.** A video layer is labelled with its file's name instead of "Video".
Right-clicking any layer offers:
- Bring to Front, Bring Forward, Send Backward and Send to Back (later in the list draws
  on top);
- for a video, Full Frame (fills the picture, cropping if its shape differs), Picture in
  Picture (a small window low on the right), and Mute or Unmute its sound;
- Duplicate and Delete.

## Picture shape

**Choosing a shape.** The header has a shape menu: 16:9, 9:16, 1:1, 4:5, 4:3, 3:2 and 21:9.
A new shape keeps the picture's long side, so a 4K project stays 4K.

**Fit and Fill.** Beside the menu, Fit and Fill decide how a clip of another shape sits:
- Fit shows all of it, with bars;
- Fill enlarges it to fill the frame and crops the edges (`overlay::cover`), for Reels,
  TikTok and Shorts.

Playback and export use the same composer, so they agree.

**The paused picture.** It used to show the open clip in its own shape. It is now framed to
the project's shape first (`player::frame_to`), so titles sit where they will in the video.
Mask editing and Before and After keep the clip's own shape so their overlays line up.

## GIF export

GIF is the fourth format in Export:
- silent and looping, at 15 frames a second;
- 1080, 720 or 480 on the long side;
- one palette made for the whole video, dithered with sierra2_4a.

Export builds only the frames the GIF keeps. On the test clip (11.5 seconds), the GIF came
out at 720 × 404 with 173 frames, in 9.4 seconds. A single clip exported as a GIF drops its
sound too. `lookroom --edit-test OUT.gif CLIPS` makes one from the command line.

## Export speed

Measured on this laptop (i7-1255U with Intel Iris Xe), exporting a 19-second, two-clip
720p project with a dissolve, an effect and a title (`lookroom --edit-test OUT CLIPS`):
- before: 39.7 seconds;
- after: 12.6 seconds, 45 frames a second.

The picture matches the old export at 44.9 dB PSNR, and the file is 28% smaller. The
changes, largest first:
- **Graphics chip encoding.** Fast Export, on by default, encodes on NVIDIA NVENC, Intel
  Quick Sync, AMD AMF or Apple VideoToolbox. `video::chip_for` tries each on a few small
  frames the first time it's needed and remembers the answer; ffmpeg builds list them all
  whether or not the chip is there. Without a chip it uses quicker software settings
  (`veryfast`). If a chip fails partway through a file, the export starts again in software.
  Graphics chips don't write 10-bit H.264, so that choice stays in software. Turning Fast
  Export off brings back the slower settings for the smallest file.
- **Staged colour tables.** A look's per-pixel colour steps cost about 100 ms a 720p frame
  worked out exactly. Export now bakes the steps before clarity and the steps after it into
  separate 65-point tables, so their order with sharpening, bloom and the rest is unchanged.
  - At bake time each cell's centre is checked against the exact answer. Where a step bends
    too sharply, such as the shadow lift on dark, saturated colours, that cell and its
    neighbours are worked out exactly. So are pixels beyond 0 to 1.
  - Across every built-in look, no pixel differs by as much as one 8-bit step; the average
    is under half a 10-bit step. A test holds it to that.
  - 20 ms a frame instead of 100. Playback's single, coarser table is unchanged.
- **Untouched clips skip grading** (about 11 ms a frame).
- **Grading and writing overlap.** A feeder thread turns finished frames into bytes and
  writes them while the next frame is graded. An 8-bit file is sent as 8-bit, half the bytes.
- **The next clip starts decoding** before the cut, as in playback.
- **Quicker colour conversion.** Fast Export drops `full_chroma_int` from the conversion to
  YUV, several times quicker at 1080p for a difference too small to see.

`lookroom --bench CLIP` prints the timing of each step, and `--edit-test` prints the export
speed beside the speed of making the frames alone.

## What this app does not copy

The colour tables are this app's own, calculated by the recipes in
`src/looks.rs`, because permission to reuse the original's table files has not
been confirmed. Looks follow the original's settings but will not match it pixel
for pixel.
