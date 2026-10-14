---
title: "3D graphs — one flat monitor, or two eyes (stereo and VR headsets)"
description: The 3D graph view planned for two cases. Most people have one ordinary monitor, where 3D is a perspective view that must stay readable. Some have stereo hardware, such as a VR headset like Valve's (OpenXR, SteamVR) or a stereo display, where each eye gets a slightly different view. What the engine needs (one camera per eye, frame pacing), how interaction changes, comfort rules, and the steps. Planned 2026-10-14.
tags: [editors, graph, 3d, vr, xr, spec, planned]
---
**From Daniel (2026-10-14):** for the 3D version of the graph, assume two cases later:
- people have just one 2D monitor;
- or they see two slightly different views, one per eye, as with VR headsets such as Valve's Steam hardware.

**Today:** 3D is a mode of the same renderer (Milestone 8). It draws nodes with a depth per kind, a perspective camera orbiting the layout (`camera.rs`: one view, one projection), depth-tested and fogged edges, and node sizes attenuated with distance.

## Case 1: one flat monitor

The 3D view has to earn its place against the 2D one: depth is only useful if it carries meaning. So:
- **Depth means something:** z comes from a property chosen in the view (a layer per kind, as now; a time axis for history or runs; an IR level for the code graph, [[North Star]] §3), never just "spread out".
- **Depth cues** on a flat screen: perspective, fog (exists), size attenuation (exists), occlusion, **motion parallax** (a slight automatic orbit while idle, off by default, and orbiting with the mouse), and a ground grid that shows where z = 0 is.
- **Readability:** labels stay screen-aligned and readable at any angle (billboards), and the 2D overlay keeps working (the overlay already projects through the camera).
- **Navigation:** orbit, pan and dolly with the mouse (middle-drag pans, as in 2D); **click to fly** to a node (smooth, short); "reset view"; and the left-hand layout (WASD + Q/E, [[Input Layouts]]) moves the camera.
- **One switch between 2D and 3D,** with the camera and selection kept.

## Case 2: two eyes (stereo)

Each eye gets its own view from a slightly different position: the same scene drawn twice, with each eye's camera offset by half the eye distance.

**The hardware, in order of how it reaches Moonkale:**
- **VR headsets through OpenXR:** SteamVR (Valve's headsets) and Monado on Linux, Meta and others on Windows. OpenXR is the standard API: it gives the eyes' poses and projections every frame, and Moonkale renders into its swapchains. This means native desktop only (wgpu on Vulkan, sharing images with the OpenXR runtime).
- **WebXR in the browser:** the same idea for the web build, in browsers that support it (Chromium-based ones with a headset, and the headsets' own browsers).
- **Stereo displays and other fallbacks:** side-by-side and over-under output for stereo monitors and 3D TVs, and anaglyph (red/cyan glasses) as a cheap test mode.

**What the engine needs:**
- **A camera per view:** the camera gains `views: [View { pose, projection, viewport }]`, one for a monitor and two for stereo. Each frame draws the scene once per view (or once with multiview, where the GPU supports it).
- **Frame pacing:** headsets need 90 Hz or more, steady. A missed frame is visible as judder and is uncomfortable, so in XR the engine's level of detail adapts to keep the frame budget (11 ms at 90 Hz for both eyes), and layout steps run off the frame path.
- **Labels in 3D,** as panels in space that face the viewer, at a readable size and distance. The 2D overlay doesn't exist in a headset; text is rendered into the 3D scene (a glyph atlas on the GPU).
- **Picking with a ray** (a controller or the hand), not a screen point. The engine's picking gains a ray query beside the screen one.

**Interaction in a headset:**
- **Grab the world:** grip and drag moves the whole graph, two hands scale and rotate it. Moving the graph instead of the person is the most comfortable navigation.
- **Point and select:** a controller ray (or a hand pinch) selects a node and shows its card (label, kind, properties) beside it.
- **Teleport** to a node for large graphs, instead of flying (smooth flight makes people sick).
- **A wrist or hand menu** with a few actions: Local ↔ Whole, filters, layers, reset. Text input (search) through the headset's system keyboard or voice, or by taking the headset off: the desktop app keeps running, with the same session.
- **Linked to the desktop:** selecting a node in the headset selects it in the desktop's panels ([[Linked Panels]]), so a person can explore in 3D and read the content on the monitor, or two people can share one session.

**Comfort rules (non-negotiable in XR):**
- the camera never moves unless the user moves it;
- no acceleration the user didn't cause;
- teleport and grab instead of flying;
- a stable floor or grid;
- the frame rate before the detail;
- a "comfort mode" that vignettes during motion.

## Steps

1. **Case 1, polished:** meaningful z from a chosen property, the ground grid, billboard labels, click-to-fly, the switch keeping camera and selection.
2. **The camera with several views,** with side-by-side and anaglyph output as the first stereo, testable on any monitor.
3. **Text in the 3D scene** (the glyph atlas) and ray picking.
4. **OpenXR on the native desktop** (SteamVR and Monado): swapchains, poses, grab-the-world, point-and-select, teleport, comfort mode, frame pacing at 90 Hz.
5. **WebXR** for the web build.
6. **Linked to the desktop session,** and shared sessions in XR.

**Measured with [[Benchmarks]]:**
- BM-11's 3D variant: frames per second at 100 k nodes;
- an XR budget: 90 Hz with both eyes at 10 k and 100 k nodes on the RTX 3080, frame-time p99 under 11 ms.

**Hardware:** this needs a headset on the development machine for step 4 onwards. Until then, steps 1 to 3 are all on a monitor.

Work package: WP-62 in [[Work Packages]].
