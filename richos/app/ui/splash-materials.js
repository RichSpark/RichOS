// THE MATERIALS (round 74, approved by the CEO on 2026-11-14): five of round 8.1's material
// studies as library entries, in the library's own shape. The openings' materials ladder is
// built on them (`openings-catalog.js`, family "material"). Written by round 74's
// build/materials.mjs from richos 313ef6b3f; do not edit by hand.
//
// ONE VALUE IN EACH ENTRY IS NOT THE STUDY'S: `tagline`, the color of the line under the mark.
// The studies carry rgba(76,96,135,0.95), which on these mats is 1.75:1 to 2.78:1 measured on
// the glass, against a floor of 4.5:1 for the one line of text the screen has. It is the app's
// own line color here, #8FA3C6 (the two approved splash screens'): 5.41:1 on the leather,
// 6.11:1 on the steel, 7.10:1 on the enamel and 5.79:1 under the gold leaf. On the slate, the
// lightest of the five mats, #8FA3C6 is 4.39:1, so its line is #A3B4D2: 5.27:1. Measured from
// photographs of the finished frame against the lightest ground under the line
// (`tests/openings.js`, check 16, which holds every one of them).
//
// The shell reads this file as well as the page (`splash_pick.rs`), which is why everything
// after the assignment is plain JSON.
window.RichSplashMaterials = [
  {
    "id": "round-8.1/v8",
    "name": "Bridle leather, the mark blocked in",
    "source": "richos-hq design/mockups/rounds/round-8.1/v8/index.html",
    "tokens": {
      "ground": "#0C1322",
      "atmosphere": "none",
      "vignette": "radial-gradient(120% 90% at 50% 38%, rgba(0,0,0,0) 46%, rgba(0,0,0,.42) 100%)",
      "grainOpacity": "0.07",
      "lamp": "rgba(255,232,190,.09)",
      "lampRadius": "1050px",
      "lampStop": "66%",
      "sheen": null,
      "materials": [
        {
          "background": "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='520' height='440'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.014 0.018' numOctaves='4' seed='57' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\") 0 0 / 520px 440px",
          "blend": "soft-light",
          "opacity": ".4"
        },
        {
          "background": "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='720' height='600'%3E%3Cfilter id='n' x='0' y='0' width='100%25' height='100%25'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.045 0.055' numOctaves='3' seed='13' stitchTiles='stitch' result='t'/%3E%3CfeDiffuseLighting in='t' lighting-color='white' surfaceScale='2.6' diffuseConstant='1'%3E%3CfeDistantLight azimuth='235' elevation='58'/%3E%3C/feDiffuseLighting%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\") 0 0 / 720px 600px",
          "blend": "soft-light",
          "opacity": ".5"
        },
        {
          "background": "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='180' height='180'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.45' numOctaves='2' seed='5' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\") 0 0 / 180px 180px",
          "blend": "overlay",
          "opacity": ".16"
        },
        {
          "background": "linear-gradient(180deg,rgba(255,246,225,.05),transparent 32%)"
        },
        {
          "background": "none",
          "inset": "8px",
          "radius": "6px",
          "border": "1px solid rgba(0,0,0,.38)"
        },
        {
          "background": "none",
          "inset": "9px 8px 7px 8px",
          "radius": "6px",
          "border": "1px solid rgba(255,246,220,.045)"
        }
      ],
      "surface": "#111B33",
      "surfaceImage": "radial-gradient(120% 105% at 50% 40%,#182642 0%,#111B33 55%,#0C1428 100%)",
      "plinthRadius": "9px",
      "plinthShadow": "inset 0 1px 0 rgba(255,255,255,.05),inset 0 0 44px rgba(0,0,0,.38),0 2px 6px rgba(0,0,0,.5),0 40px 90px -30px rgba(0,0,0,.75),0 100px 170px -50px rgba(0,0,0,.85)",
      "plinthOutline": "rgba(70,88,126,.4)",
      "keylineInset": "13px",
      "keylineRadius": "4px",
      "keylineWidth": "1px",
      "keylineStyle": "solid",
      "keylineColor": "rgba(194,163,92,.30)",
      "keylineImage": "none",
      "keylineShadow": "none",
      "ink": "#DFE4EE",
      "signal": "#C2A35C",
      "gild": null,
      "markFilter": null,
      "signalFilter": null,
      "relief": {
        "target": "mark",
        "region": "-20% -20% 140% 140%",
        "noise": null,
        "matrix": null,
        "bands": [
          {
            "color": "#000008",
            "opacity": ".55",
            "dx": "0",
            "dy": "2.2",
            "blur": "1.4",
            "placement": "inner"
          },
          {
            "color": "#FFF3DA",
            "opacity": ".26",
            "dx": "0",
            "dy": "-1.6",
            "blur": "1.2",
            "placement": "inner"
          }
        ]
      },
      "rule": "#C2A35C",
      "ruleWidth": "58px",
      "tagline": "#8FA3C6",
      "riseDuration": "0.9s",
      "riseShift": "16px",
      "riseDelays": [
        "0.05s",
        "0.18s",
        "0.32s",
        "0.48s"
      ],
      "strike": "none",
      "strikeFrom": null,
      "strikePeak": null,
      "strikeDelay": "0s",
      "strikeDuration": "0s",
      "seconds": null,
      "bar": {
        "gap": "clamp(30px, 4.6vh, 46px)",
        "height": "2px",
        "radius": "1px",
        "background": "rgba(194,163,92,0.22)",
        "outline": "none",
        "shadow": "none",
        "clip": "visible",
        "enterDelay": "0.62s",
        "enterDuration": "0.7s",
        "pitch": null,
        "pitchInset": "0px",
        "layers": [
          {
            "role": "progress",
            "background": "linear-gradient(to bottom, rgba(194,163,92,.10), rgba(194,163,92,0) 72%)",
            "inset": "2px auto auto 0",
            "height": "26px",
            "mask": "linear-gradient(to right, rgba(0,0,0,0) 0, #000 8%, #000 92%, rgba(0,0,0,0) 100%)"
          },
          {
            "role": "progress",
            "background": "#C2A35C",
            "inset": "-1px auto -1px 0",
            "radius": "2px",
            "shadow": "0 0 10px rgba(194,163,92,.34), 0 0 2px rgba(194,163,92,.85)"
          },
          {
            "role": "lead",
            "background": "radial-gradient(circle, rgba(214,182,110,.30) 0%, rgba(194,163,92,.13) 34%, rgba(194,163,92,0) 68%)",
            "inset": "50% -1px auto auto",
            "width": "74px",
            "height": "74px",
            "transform": "translate(50%, -50%)"
          },
          {
            "role": "flare",
            "background": "linear-gradient(100deg, rgba(255,240,205,0) 40%, rgba(255,240,205,.85) 50%, rgba(255,240,205,0) 60%) 130% 0 / 260% 100%",
            "inset": "-3px 0",
            "radius": "3px",
            "opacity": "0.9"
          }
        ]
      }
    }
  },
  {
    "id": "round-8.1/v11",
    "name": "Honed slate, the mark cut in and gilded",
    "source": "richos-hq design/mockups/rounds/round-8.1/v11/index.html",
    "tokens": {
      "ground": "#0C1322",
      "atmosphere": "none",
      "vignette": "radial-gradient(120% 90% at 50% 38%, rgba(0,0,0,0) 46%, rgba(0,0,0,.42) 100%)",
      "grainOpacity": "0.07",
      "lamp": "rgba(200,215,255,.12)",
      "lampRadius": "1050px",
      "lampStop": "66%",
      "sheen": null,
      "materials": [
        {
          "background": "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='900' height='600'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.004 0.02' numOctaves='4' seed='17' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\") 0 0 / 900px 600px",
          "blend": "soft-light",
          "opacity": ".62"
        },
        {
          "background": "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='340' height='300'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.09 0.13' numOctaves='3' seed='23' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\") 0 0 / 340px 300px",
          "blend": "overlay",
          "opacity": ".28"
        },
        {
          "background": "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.8' numOctaves='1' seed='29' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\") 0 0 / 160px 160px",
          "blend": "screen",
          "opacity": ".3",
          "filter": "contrast(3) brightness(.6)"
        },
        {
          "background": "none",
          "inset": "14px 13px 12px 13px",
          "radius": "3px",
          "border": "1px solid rgba(223,228,238,.05)"
        }
      ],
      "surface": "#131C31",
      "surfaceImage": "none",
      "plinthRadius": "4px",
      "plinthShadow": "inset 0 1px 0 rgba(255,255,255,.05),0 3px 7px rgba(0,0,0,.55),0 44px 90px -28px rgba(0,0,0,.8),0 110px 180px -50px rgba(0,0,0,.88)",
      "plinthOutline": "rgba(90,106,140,.3)",
      "keylineInset": "13px",
      "keylineRadius": "4px",
      "keylineWidth": "1px",
      "keylineStyle": "solid",
      "keylineColor": "rgba(0,0,0,.5)",
      "keylineImage": "none",
      "keylineShadow": "none",
      "ink": "#DFE4EE",
      "signal": "#C2A35C",
      "gild": null,
      "markFilter": null,
      "signalFilter": null,
      "relief": {
        "target": "mark",
        "region": "-20% -20% 140% 140%",
        "noise": null,
        "matrix": null,
        "bands": [
          {
            "color": "#000006",
            "opacity": ".72",
            "dx": "0",
            "dy": "3.2",
            "blur": "2",
            "placement": "inner"
          },
          {
            "color": "#EDF2FF",
            "opacity": ".3",
            "dx": "0",
            "dy": "-1.8",
            "blur": "1.3",
            "placement": "inner"
          }
        ]
      },
      "rule": "#C2A35C",
      "ruleWidth": "58px",
      "tagline": "#A3B4D2",
      "riseDuration": "0.9s",
      "riseShift": "16px",
      "riseDelays": [
        "0.05s",
        "0.18s",
        "0.32s",
        "0.48s"
      ],
      "strike": "none",
      "strikeFrom": null,
      "strikePeak": null,
      "strikeDelay": "0s",
      "strikeDuration": "0s",
      "seconds": null,
      "bar": {
        "gap": "clamp(30px, 4.6vh, 46px)",
        "height": "2px",
        "radius": "1px",
        "background": "rgba(194,163,92,0.22)",
        "outline": "none",
        "shadow": "none",
        "clip": "visible",
        "enterDelay": "0.62s",
        "enterDuration": "0.7s",
        "pitch": null,
        "pitchInset": "0px",
        "layers": [
          {
            "role": "progress",
            "background": "linear-gradient(to bottom, rgba(194,163,92,.10), rgba(194,163,92,0) 72%)",
            "inset": "2px auto auto 0",
            "height": "26px",
            "mask": "linear-gradient(to right, rgba(0,0,0,0) 0, #000 8%, #000 92%, rgba(0,0,0,0) 100%)"
          },
          {
            "role": "progress",
            "background": "#C2A35C",
            "inset": "-1px auto -1px 0",
            "radius": "2px",
            "shadow": "0 0 10px rgba(194,163,92,.34), 0 0 2px rgba(194,163,92,.85)"
          },
          {
            "role": "lead",
            "background": "radial-gradient(circle, rgba(214,182,110,.30) 0%, rgba(194,163,92,.13) 34%, rgba(194,163,92,0) 68%)",
            "inset": "50% -1px auto auto",
            "width": "74px",
            "height": "74px",
            "transform": "translate(50%, -50%)"
          },
          {
            "role": "flare",
            "background": "linear-gradient(100deg, rgba(255,240,205,0) 40%, rgba(255,240,205,.85) 50%, rgba(255,240,205,0) 60%) 130% 0 / 260% 100%",
            "inset": "-3px 0",
            "radius": "3px",
            "opacity": "0.9"
          }
        ]
      }
    }
  },
  {
    "id": "round-8.1/v13",
    "name": "Blued steel, the mark inlaid in gold",
    "source": "richos-hq design/mockups/rounds/round-8.1/v13/index.html",
    "tokens": {
      "ground": "#0C1322",
      "atmosphere": "none",
      "vignette": "radial-gradient(120% 90% at 50% 38%, rgba(0,0,0,0) 46%, rgba(0,0,0,.42) 100%)",
      "grainOpacity": "0.07",
      "lamp": "rgba(205,222,255,.09)",
      "lampRadius": "1000px",
      "lampStop": "66%",
      "sheen": null,
      "materials": [
        {
          "background": "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='1200' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.002 0.5' numOctaves='2' seed='37' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\") 0 0 / 1200px 160px",
          "blend": "overlay",
          "opacity": ".5"
        },
        {
          "background": "repeating-linear-gradient(180deg,rgba(255,255,255,.05) 0 1px,transparent 1px 3px)",
          "opacity": ".22"
        },
        {
          "background": "radial-gradient(820px 64px at 29% 34%, rgba(205,222,255,.11), rgba(0,0,0,0) 70%)",
          "blend": "screen"
        },
        {
          "background": "none",
          "inset": "17px",
          "radius": "2px",
          "border": "1px solid rgba(194,163,92,.18)"
        }
      ],
      "surface": "#101A31",
      "surfaceImage": "linear-gradient(180deg,#16213A 0%,#101A31 55%,#141F38 100%)",
      "plinthRadius": "5px",
      "plinthShadow": "inset 0 1px 0 rgba(255,255,255,.07),0 1px 3px rgba(0,0,0,.6),0 34px 76px -28px rgba(0,0,0,.78),0 90px 150px -46px rgba(0,0,0,.85)",
      "plinthOutline": "rgba(120,140,180,.22)",
      "keylineInset": "13px",
      "keylineRadius": "4px",
      "keylineWidth": "1px",
      "keylineStyle": "solid",
      "keylineColor": "rgba(194,163,92,.45)",
      "keylineImage": "none",
      "keylineShadow": "none",
      "ink": "#DFE4EE",
      "signal": "#C2A35C",
      "gild": null,
      "markFilter": null,
      "signalFilter": "drop-shadow(0 0 0.7px rgba(0,0,0,.85))",
      "relief": null,
      "rule": "#C2A35C",
      "ruleWidth": "58px",
      "tagline": "#8FA3C6",
      "riseDuration": "0.9s",
      "riseShift": "16px",
      "riseDelays": [
        "0.05s",
        "0.18s",
        "0.32s",
        "0.48s"
      ],
      "strike": "none",
      "strikeFrom": null,
      "strikePeak": null,
      "strikeDelay": "0s",
      "strikeDuration": "0s",
      "seconds": null,
      "bar": {
        "gap": "clamp(30px, 4.6vh, 46px)",
        "height": "2px",
        "radius": "1px",
        "background": "rgba(194,163,92,0.22)",
        "outline": "none",
        "shadow": "none",
        "clip": "visible",
        "enterDelay": "0.62s",
        "enterDuration": "0.7s",
        "pitch": null,
        "pitchInset": "0px",
        "layers": [
          {
            "role": "progress",
            "background": "linear-gradient(to bottom, rgba(194,163,92,.10), rgba(194,163,92,0) 72%)",
            "inset": "2px auto auto 0",
            "height": "26px",
            "mask": "linear-gradient(to right, rgba(0,0,0,0) 0, #000 8%, #000 92%, rgba(0,0,0,0) 100%)"
          },
          {
            "role": "progress",
            "background": "#C2A35C",
            "inset": "-1px auto -1px 0",
            "radius": "2px",
            "shadow": "0 0 10px rgba(194,163,92,.34), 0 0 2px rgba(194,163,92,.85)"
          },
          {
            "role": "lead",
            "background": "radial-gradient(circle, rgba(214,182,110,.30) 0%, rgba(194,163,92,.13) 34%, rgba(194,163,92,0) 68%)",
            "inset": "50% -1px auto auto",
            "width": "74px",
            "height": "74px",
            "transform": "translate(50%, -50%)"
          },
          {
            "role": "flare",
            "background": "linear-gradient(100deg, rgba(255,240,205,0) 40%, rgba(255,240,205,.85) 50%, rgba(255,240,205,0) 60%) 130% 0 / 260% 100%",
            "inset": "-3px 0",
            "radius": "3px",
            "opacity": "0.9"
          }
        ]
      }
    }
  },
  {
    "id": "round-8.1/v16",
    "name": "Guilloché under midnight enamel",
    "source": "richos-hq design/mockups/rounds/round-8.1/v16/index.html",
    "tokens": {
      "ground": "#0C1322",
      "atmosphere": "none",
      "vignette": "radial-gradient(120% 90% at 50% 38%, rgba(0,0,0,0) 46%, rgba(0,0,0,.42) 100%)",
      "grainOpacity": "0.07",
      "lamp": "rgba(190,210,255,.10)",
      "lampRadius": "950px",
      "lampStop": "66%",
      "sheen": null,
      "materials": [
        {
          "background": "repeating-radial-gradient(circle at 32% 40%,rgba(190,205,240,.04) 0 1px,transparent 1px 6px), repeating-radial-gradient(circle at 68% 60%,rgba(190,205,240,.04) 0 1px,transparent 1px 6px)",
          "opacity": ".9"
        },
        {
          "background": "linear-gradient(160deg,rgba(24,36,66,.36),rgba(8,14,30,.5))"
        },
        {
          "background": "repeating-radial-gradient(circle at 32% 40%,rgba(220,232,255,.16) 0 1px,transparent 1px 6px), repeating-radial-gradient(circle at 68% 60%,rgba(220,232,255,.16) 0 1px,transparent 1px 6px)",
          "mask": "radial-gradient(340px circle at 29% 34%, rgba(0,0,0,1) 0%, rgba(0,0,0,0) 70%)"
        },
        {
          "background": "linear-gradient(103deg,transparent 30%,rgba(170,190,235,.05) 43%,rgba(190,208,245,.08) 47%,transparent 60%)",
          "blend": "screen"
        }
      ],
      "surface": "#0C1428",
      "surfaceImage": "linear-gradient(160deg,#101B36 0%,#0C1428 55%,#0E1730 100%)",
      "plinthRadius": "6px",
      "plinthShadow": "inset 0 0 0 1px rgba(150,170,210,.08),inset 0 0 34px rgba(0,0,0,.4),0 1px 3px rgba(0,0,0,.6),0 32px 72px -26px rgba(0,0,0,.8),0 90px 150px -45px rgba(0,0,0,.85)",
      "plinthOutline": "rgba(0,0,0,0)",
      "keylineInset": "13px",
      "keylineRadius": "3px",
      "keylineWidth": "2px",
      "keylineStyle": "solid",
      "keylineColor": "rgba(0,0,0,0)",
      "keylineImage": "linear-gradient(180deg,#E2C87E,#8a6f35) 1",
      "keylineShadow": "none",
      "ink": "#DFE4EE",
      "signal": "#C2A35C",
      "gild": null,
      "markFilter": null,
      "signalFilter": "drop-shadow(0 1px 1px rgba(0,0,0,.55))",
      "relief": null,
      "rule": "#C2A35C",
      "ruleWidth": "58px",
      "tagline": "#8FA3C6",
      "riseDuration": "0.9s",
      "riseShift": "16px",
      "riseDelays": [
        "0.05s",
        "0.18s",
        "0.32s",
        "0.48s"
      ],
      "strike": "none",
      "strikeFrom": null,
      "strikePeak": null,
      "strikeDelay": "0s",
      "strikeDuration": "0s",
      "seconds": null,
      "bar": {
        "gap": "clamp(30px, 4.6vh, 46px)",
        "height": "2px",
        "radius": "1px",
        "background": "rgba(194,163,92,0.22)",
        "outline": "none",
        "shadow": "none",
        "clip": "visible",
        "enterDelay": "0.62s",
        "enterDuration": "0.7s",
        "pitch": null,
        "pitchInset": "0px",
        "layers": [
          {
            "role": "progress",
            "background": "linear-gradient(to bottom, rgba(194,163,92,.10), rgba(194,163,92,0) 72%)",
            "inset": "2px auto auto 0",
            "height": "26px",
            "mask": "linear-gradient(to right, rgba(0,0,0,0) 0, #000 8%, #000 92%, rgba(0,0,0,0) 100%)"
          },
          {
            "role": "progress",
            "background": "#C2A35C",
            "inset": "-1px auto -1px 0",
            "radius": "2px",
            "shadow": "0 0 10px rgba(194,163,92,.34), 0 0 2px rgba(194,163,92,.85)"
          },
          {
            "role": "lead",
            "background": "radial-gradient(circle, rgba(214,182,110,.30) 0%, rgba(194,163,92,.13) 34%, rgba(194,163,92,0) 68%)",
            "inset": "50% -1px auto auto",
            "width": "74px",
            "height": "74px",
            "transform": "translate(50%, -50%)"
          },
          {
            "role": "flare",
            "background": "linear-gradient(100deg, rgba(255,240,205,0) 40%, rgba(255,240,205,.85) 50%, rgba(255,240,205,0) 60%) 130% 0 / 260% 100%",
            "inset": "-3px 0",
            "radius": "3px",
            "opacity": "0.9"
          }
        ]
      }
    }
  },
  {
    "id": "round-8.1/v17",
    "name": "Oil and gold leaf, under a picture light",
    "source": "richos-hq design/mockups/rounds/round-8.1/v17/index.html",
    "tokens": {
      "ground": "#070C17",
      "atmosphere": "none",
      "vignette": "radial-gradient(120% 90% at 50% 34%,rgba(0,0,0,0) 34%,rgba(0,0,0,.6) 100%)",
      "grainOpacity": "0.07",
      "lamp": "rgba(255,228,168,.06)",
      "lampRadius": "900px",
      "lampStop": "66%",
      "sheen": null,
      "materials": [
        {
          "background": "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.55' numOctaves='2' seed='53' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\") 0 0 / 160px 160px, repeating-linear-gradient(0deg,rgba(255,255,255,.016) 0 1px,transparent 1px 3px) 0 0 / auto, linear-gradient(180deg,#131E37,#0E1730) 0 0 / cover",
          "imageBlend": "soft-light,normal,normal",
          "inset": "11px",
          "radius": "1px"
        },
        {
          "background": "radial-gradient(135% 100% at 50% -8%,rgba(255,224,158,.46),rgba(255,220,152,.14) 44%,transparent 72%)",
          "blend": "screen",
          "inset": "11px",
          "radius": "1px"
        },
        {
          "background": "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.55' numOctaves='2' seed='53' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\") 0 0 / 160px 160px",
          "blend": "soft-light",
          "opacity": ".1",
          "inset": "11px",
          "radius": "1px",
          "z": "3"
        },
        {
          "background": "radial-gradient(50% 100% at 50% 0%,rgba(255,228,168,.28),transparent 78%)",
          "inset": "-8px 34.88% auto 34.88%",
          "height": "26px",
          "radius": "50%"
        },
        {
          "background": "linear-gradient(180deg,#5c4a20,#8a6f35)",
          "inset": "-25px calc(50% - 13px) auto calc(50% - 13px)",
          "height": "8px",
          "radius": "3px 3px 0 0"
        },
        {
          "background": "linear-gradient(180deg,#EAD394,#8a6f35)",
          "inset": "-17px 32% auto 32%",
          "height": "8px",
          "radius": "4px",
          "shadow": "0 2px 6px rgba(0,0,0,.55)"
        }
      ],
      "surface": "#A2854A",
      "surfaceImage": "linear-gradient(178deg,#D9BC72 0%,#A2854A 22%,#C8AA5F 50%,#6E5626 100%)",
      "plinthRadius": "3px",
      "plinthShadow": "inset 0 0 0 1px rgba(60,45,10,.6),inset 0 0 0 3px rgba(255,240,200,.22),inset 0 0 0 4px rgba(0,0,0,.25), 0 3px 8px rgba(0,0,0,.55),0 46px 95px -30px rgba(0,0,0,.85),0 110px 180px -50px rgba(0,0,0,.9)",
      "plinthOutline": "rgba(0,0,0,0)",
      "keylineInset": "11px",
      "keylineRadius": "1px",
      "keylineWidth": "0px",
      "keylineStyle": "solid",
      "keylineColor": "rgba(0,0,0,0)",
      "keylineImage": "none",
      "keylineShadow": "inset 0 5px 10px -3px rgba(0,0,0,.62),inset 0 -3px 8px -4px rgba(0,0,0,.42),inset 4px 0 8px -4px rgba(0,0,0,.4),inset -4px 0 8px -4px rgba(0,0,0,.4)",
      "ink": "#DFE4EE",
      "signal": "#C2A35C",
      "gild": null,
      "markFilter": null,
      "signalFilter": null,
      "relief": null,
      "rule": "#C2A35C",
      "ruleWidth": "58px",
      "tagline": "#8FA3C6",
      "riseDuration": "0.9s",
      "riseShift": "16px",
      "riseDelays": [
        "0.05s",
        "0.18s",
        "0.32s",
        "0.48s"
      ],
      "strike": "none",
      "strikeFrom": null,
      "strikePeak": null,
      "strikeDelay": "0s",
      "strikeDuration": "0s",
      "seconds": null,
      "bar": {
        "gap": "clamp(30px, 4.6vh, 46px)",
        "height": "2px",
        "radius": "1px",
        "background": "rgba(194,163,92,0.22)",
        "outline": "none",
        "shadow": "none",
        "clip": "visible",
        "enterDelay": "0.62s",
        "enterDuration": "0.7s",
        "pitch": null,
        "pitchInset": "0px",
        "layers": [
          {
            "role": "progress",
            "background": "linear-gradient(to bottom, rgba(194,163,92,.10), rgba(194,163,92,0) 72%)",
            "inset": "2px auto auto 0",
            "height": "26px",
            "mask": "linear-gradient(to right, rgba(0,0,0,0) 0, #000 8%, #000 92%, rgba(0,0,0,0) 100%)"
          },
          {
            "role": "progress",
            "background": "#C2A35C",
            "inset": "-1px auto -1px 0",
            "radius": "2px",
            "shadow": "0 0 10px rgba(194,163,92,.34), 0 0 2px rgba(194,163,92,.85)"
          },
          {
            "role": "lead",
            "background": "radial-gradient(circle, rgba(214,182,110,.30) 0%, rgba(194,163,92,.13) 34%, rgba(194,163,92,0) 68%)",
            "inset": "50% -1px auto auto",
            "width": "74px",
            "height": "74px",
            "transform": "translate(50%, -50%)"
          },
          {
            "role": "flare",
            "background": "linear-gradient(100deg, rgba(255,240,205,0) 40%, rgba(255,240,205,.85) 50%, rgba(255,240,205,0) 60%) 130% 0 / 260% 100%",
            "inset": "-3px 0",
            "radius": "3px",
            "opacity": "0.9"
          }
        ]
      }
    }
  }
];
