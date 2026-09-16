# NusaHarvest Siaga Tanam: 90-second video script

Format: 1920x1080, 30 fps, 2700 frames. Palette: #1f3d2b green, #c9962e gold, #f3f0e8 paper. Vector and CSS motion only, no stock footage. Voiceover in English, Indonesian subtitles optional.

All numbers come from `research/data/backtest-calibrated.json`. The video does not state or imply any users, sponsors or funds.

## Shot list

| # | Time | Visual | Voiceover |
|---|------|--------|-----------|
| 1 | 0:00-0:08 | Paper background. A thin green line draws a rice field horizon. Title fades in: "When the rain is late, planting money is late too." | "In Central Java, rice farmers plant when the October rain comes. Some years, it doesn't." |
| 2 | 0:08-0:18 | A rainfall bar for Grobogan fills slowly and stops short of a gold threshold line. Label: 2023, 402 mm. Threshold: 559 mm. | "In 2023, Grobogan got 402 millimetres from October to December. Its normal low-year mark is 559." |
| 3 | 0:18-0:28 | Three icons in a row: coin, list, arrow, each passing through a grey box labelled "intermediary". The box flickers and coins drop out. | "Help usually passes through several hands first. Lists change, money stalls, and by then the planting window is gone." |
| 4 | 0:28-0:40 | Vault graphic locks with a click. Label: "Sponsor locks its own funds before the season." Rows of phone numbers compress into a single hash, stamped "roster sealed". | "Siaga Tanam is a conditional grant. A sponsor locks its own funds on Solana before the season. The farmer list is sealed as a merkle root, so nobody gets added after a drought." |
| 5 | 0:40-0:55 | Map outline of four districts. Satellite icon. Counter ticks Oct 1 to Dec 31. Each district shows observed mm against its own 20th percentile. | "After December, satellite rainfall from ERA5 is compared with each district's own 20th percentile. The number and a hash of the source data go on-chain." |
| 6 | 0:55-1:05 | Timer ring counts 48 hours. Auditor badge sits beside it. Ring completes, vault opens, gold line flows to a "licensed payment partner" box, then to phones showing a rupiah e-wallet balance. | "An auditor has 48 hours to dispute. Then the grant is paid in rupiah to each farmer's e-wallet through a licensed payment partner. Farmers pay nothing and never touch crypto." |
| 7 | 1:05-1:18 | Backtest chart: Grobogan trigger years 2002, 2004, 2006, 2009, 2019, 2023 as bars under the threshold. Side table: Grobogan 6, Demak 6, Klaten 5, Kupang 4 hits of 25 years. | "We backtested 25 years across four districts. The trigger fires four to six times per district, in years like 2019 and 2023, not every dry spell." |
| 8 | 1:18-1:26 | Screen capture of the landing page scrolling (recorded as video, not a still image). Status chips: "Devnet, test tokens only", "Pilot after legal opinion". | "The MVP runs on devnet with test tokens. A real-money pilot comes only after a legal opinion and a written grant agreement." |
| 9 | 1:26-1:30 | Green end card: NusaHarvest Siaga Tanam, nusaharvest-siaga.vercel.app, github.com/bryankwandou/nusaharvest-siaga | "We're looking for one sponsor to fund the first grant season with us." |

Word count of voiceover: about 210 words, which fits 90 seconds at a calm pace.

## Remotion scene breakdown

Root composition `SiagaTanam` (1920x1080, 30 fps, durationInFrames 2700). Each scene is a `<Sequence>`; transitions are 10-frame cross-fades using `interpolate` on opacity.

| Scene component | from | durationInFrames | Key animation |
|---|---|---|---|
| `<Hook />` | 0 | 240 | `evolvePath` on horizon SVG path, title `spring` from y+20 |
| `<RainBar district="Grobogan" year={2023} mm={402} p20={559} />` | 240 | 300 | Bar height `interpolate(frame,[0,120],[0,402])`, threshold line fades in at frame 20, red tint at frame 130 |
| `<Intermediary />` | 540 | 300 | Three tokens translateX through a box; `random()` jitter for flicker; tokens fall with `spring({damping:12})` |
| `<LockAndSeal />` | 840 | 360 | Vault door rotate 0 to 90 deg; list rows scale-y to 0 and merge into a monospaced hash string typed out char by char |
| `<Settle />` | 1200 | 450 | Four district cards stagger in (8 frames apart); date counter via `Math.floor(interpolate(...))`; values from JSON import |
| `<DisputeWindow />` | 1650 | 300 | SVG circle `strokeDashoffset` from full to 0 over 180 frames, then vault opens and gold path draws toward phone icons |
| `<Backtest />` | 1950 | 390 | Bars grow with 6-frame stagger; table rows fade in after bars |
| `<Status />` | 2340 | 240 | `<OffthreadVideo>` of recorded landing scroll inside a browser frame; status chips pop with `spring` |
| `<EndCard />` | 2580 | 120 | Logo and links fade in, hold |

Implementation notes:

- Import `research/data/backtest-calibrated.json` directly so chart numbers cannot drift from the data.
- Fonts: load Fraunces and Inter with `@remotion/google-fonts`.
- Voiceover as `<Audio src={staticFile('vo.mp3')} />` on the root; cut scene boundaries to the recorded VO, then adjust `from` values.
- Render: `npx remotion render SiagaTanam out/siaga-tanam.mp4 --codec h264 --crf 18`.
