#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_backpressure_reserves_before_gpu_submission() {
        let available = Arc::new(AtomicBool::new(true));
        let permit = OutputPermit::acquire(&available).unwrap();
        assert!(OutputPermit::acquire(&available).is_none());
        // Receipt/drop releases the slot even on presentation failure or exit.
        drop(permit);
        assert!(OutputPermit::acquire(&available).is_some());
    }

    #[test]
    fn app_supervisor_uses_lifecycle_focus_and_shell_painter_order() {
        let mut facts = [
            SchedulingFact {
                visible: true,
                focused: false,
                order: 10,
                state: AppInstanceState::Running,
            },
            SchedulingFact {
                visible: false,
                focused: false,
                order: 20,
                state: AppInstanceState::Suspended,
            },
            SchedulingFact {
                visible: true,
                focused: true,
                order: 30,
                state: AppInstanceState::Running,
            },
            SchedulingFact {
                visible: true,
                focused: true,
                order: 25,
                state: AppInstanceState::Failed,
            },
        ];
        assert_eq!(focused_app_instance(&facts), Some(2));
        assert_eq!(scheduled_app_instances(&facts), vec![2, 0]);
        facts[1].state = AppInstanceState::Running;
        assert_eq!(scheduled_app_instances(&facts), vec![2, 1, 0]);
    }

    #[test]
    fn app_instances_do_not_share_quickjs_globals() {
        let hero = Guest::new().unwrap();
        let settings = Guest::new().unwrap();
        hero.eval("hero", "globalThis.realmProbe = 41;").unwrap();
        settings
            .eval(
                "settings",
                "globalThis.realmProbeWasAbsent = typeof realmProbe === 'undefined';",
            )
            .unwrap();

        let hero_probe: i32 = hero.with(|ctx| ctx.globals().get("realmProbe").unwrap());
        let settings_absent: bool =
            settings.with(|ctx| ctx.globals().get("realmProbeWasAbsent").unwrap());
        assert_eq!(hero_probe, 41);
        assert!(settings_absent);
    }

    #[test]
    fn app_instance_repaint_hash_includes_raster_revision() {
        let surface = UiSurface::new((16.0, 16.0));
        let texture = surface.with_ui(|ui| {
            ui.upload_texture(
                &[0xff, 0xff, 0xff, 0xff],
                1,
                1,
                pocketjs_core::spec::psm::PSM_8888,
            )
        });
        assert!(texture >= 0);

        let (words_before, revision_before) =
            surface.with_ui(|ui| (ui.draw().words.clone(), ui.raster_revision()));
        let mut hash_before = 0xcbf2_9ce4_8422_2325u64;
        mix_app_instance_repaint_hash(&mut hash_before, 7, fnv1a64(&words_before), revision_before);

        surface.with_ui(|ui| ui.free_texture(texture));
        let (words_after, revision_after) =
            surface.with_ui(|ui| (ui.draw().words.clone(), ui.raster_revision()));
        let mut hash_after = 0xcbf2_9ce4_8422_2325u64;
        mix_app_instance_repaint_hash(&mut hash_after, 7, fnv1a64(&words_after), revision_after);

        assert_eq!(words_after, words_before);
        assert_ne!(revision_after, revision_before);
        assert_ne!(hash_after, hash_before);
    }

    #[test]
    fn resolved_system_plan_uses_the_exact_system_ui_wire_key() {
        let plan: ResolvedSystemPlan = serde_json::from_value(serde_json::json!({
            "system": {
                "id": "dev.pocket-stack.desktop",
                "name": "pocket-desktop",
                "title": "Pocket Desktop",
                "version": "0.1.0"
            },
            "target": { "id": HOST_ID, "hostAbi": 4 },
            "roles": { "systemUI": "dev.pocket-stack.shell" },
            "lifecycle": { "backgroundExecution": "suspend" },
            "installation": {
                "installedPackages": ["dev.pocket-stack.shell"]
            },
            "systemUI": {
                "package": "dev.pocket-stack.shell",
                "source": "apps/shell/pocket.json",
                "required": true,
                "plan": {
                    "app": {
                        "id": "dev.pocket-stack.shell",
                        "output": "shell-main",
                        "title": "System UI",
                        "version": "0.1.0",
                        "entry": "apps/shell/main.tsx",
                        "framework": "solid"
                    },
                    "target": { "id": HOST_ID, "hostAbi": 4 },
                    "viewport": {
                        "logical": [800, 600],
                        "physical": [1600, 1200],
                        "presentation": "native",
                        "rasterDensity": 2,
                        "policy": "dynamic"
                    },
                    "features": { "ui.compositor-surfaces": true },
                    "companions": ["system-ui"],
                    "planHash": "sha256:package"
                }
            },
            "applications": [],
            "planHash": "sha256:system"
        }))
        .unwrap();

        assert_eq!(plan.roles.system_ui, "dev.pocket-stack.shell");
        assert_eq!(plan.system_ui.package, "dev.pocket-stack.shell");
        assert!(plan.validate_for_host().is_ok());
    }

    #[test]
    fn a2_pattern_is_deterministic_and_diagnostic() {
        let same = a2_pattern(A2_W, A2_H, 0) == a2_pattern(A2_W, A2_H, 0);
        assert!(same, "the pattern must be a pure function of (w, h, variant)");
        assert_ne!(a2_pattern(64, 64, 0), a2_pattern(64, 64, 1));
        let pixels = a2_pattern(96, 64, 0);
        assert_eq!(pixels.len(), 96 * 64 * 4);
        let at = |x: u32, y: u32| -> [u8; 4] {
            let i = (y * 96 + x) as usize * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        // Opaque everywhere (no half-composited ambiguity in the proof run).
        assert!(pixels.iter().skip(3).step_by(4).all(|&a| a == 255));
        // Border, quadrant fields, center crosshair and grid lines are all
        // individually identifiable.
        assert_eq!(at(0, 0), [255, 214, 0, 255]); // variant-0 border
        assert_eq!(at(8, 8), [168, 32, 26, 255]); // top-left quadrant base
        assert_eq!(at(85, 53), [190, 140, 24, 255]); // bottom-right quadrant
        assert_eq!(at(48, 20), [250, 250, 250, 255]); // center vertical crosshair
        assert_eq!(at(12, 10), at(30, 10)); // the every-6px grid line columns
    }

    #[test]
    fn a2_harness_schedule_registers_announces_and_retires() {
        // Full state machine over a real UiSurface: bounded svc lines only,
        // native retirement drops bytes synchronously, and the schedule ends
        // with every resource retired and a boundary summary.
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A2Harness::new(true);
        for tick in 0..=721 {
            harness.tick(tick, &surface);
        }
        // The schedule's announcement count: A, B, retire A, 8 stress
        // (announce + retire), retire R7 = 20 bounded svc lines.
        assert_eq!(harness.tx_lines, 20);
        assert!(harness.live.is_empty(), "the schedule retires everything");
        let live_bytes = surface.with_ui(|ui| ui.texture_live_bytes());
        assert_eq!(live_bytes, 0, "retirement is synchronous, not GC-bound");
        assert!(harness.tx_bytes < 4096, "announcements are bounded semantic fields");
    }

    #[test]
    fn a2_stale_handles_resolve_to_absence_end_to_end() {
        let surface = UiSurface::new((64.0, 64.0));
        let mut harness = A2Harness::new(true);
        for tick in 0..=151 {
            harness.tick(tick, &surface);
        }
        // A (announced at tick 2) and B (tick 150) are live.
        assert_eq!(harness.live.len(), 2);
        let handle_a = harness.live[0].1;
        surface.with_ui(|ui| assert!(ui.texture(handle_a).is_some()));
        for tick in 152..=301 {
            harness.tick(tick, &surface);
        }
        // A was natively retired at tick 300: the handle is stale...
        assert_eq!(harness.live.len(), 1);
        surface.with_ui(|ui| assert!(ui.texture(handle_a).is_none()));
        // ...and the slot table itself shows the synchronous release.
        let slots = surface.with_ui(|ui| ui.texture_slot_count());
        assert_eq!(slots, 2);
    }

    // ------------------------------------------------------------------
    // A3 WIC-first JPEG harness. Seams: the host svc boundary (bounded
    // a3manifest / a3img / a3error lines), the native decode entry, and
    // the EXIF orientation mapping. No pixel-sized value may appear in
    // any svc line.
    // ------------------------------------------------------------------

    /// 32x16 JPEG (left red / right blue / white top rows), WIC-encoded.
    const A3_FIXTURE_JPEG: &[u8] = &[
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01,
        0x01, 0x01, 0x00, 0x60, 0x00, 0x60, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43,
        0x00, 0x03, 0x02, 0x02, 0x03, 0x02, 0x02, 0x03, 0x03, 0x03, 0x03, 0x04,
        0x03, 0x03, 0x04, 0x05, 0x08, 0x05, 0x05, 0x04, 0x04, 0x05, 0x0a, 0x07,
        0x07, 0x06, 0x08, 0x0c, 0x0a, 0x0c, 0x0c, 0x0b, 0x0a, 0x0b, 0x0b, 0x0d,
        0x0e, 0x12, 0x10, 0x0d, 0x0e, 0x11, 0x0e, 0x0b, 0x0b, 0x10, 0x16, 0x10,
        0x11, 0x13, 0x14, 0x15, 0x15, 0x15, 0x0c, 0x0f, 0x17, 0x18, 0x16, 0x14,
        0x18, 0x12, 0x14, 0x15, 0x14, 0xff, 0xdb, 0x00, 0x43, 0x01, 0x03, 0x04,
        0x04, 0x05, 0x04, 0x05, 0x09, 0x05, 0x05, 0x09, 0x14, 0x0d, 0x0b, 0x0d,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x10, 0x00, 0x20, 0x03,
        0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xff, 0xc4, 0x00,
        0x1f, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
        0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0xff, 0xc4, 0x00, 0xb5, 0x10, 0x00,
        0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00,
        0x00, 0x01, 0x7d, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21,
        0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81,
        0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0, 0x24,
        0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25,
        0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a,
        0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56,
        0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a,
        0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86,
        0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99,
        0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3,
        0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6,
        0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9,
        0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1,
        0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xff, 0xda, 0x00,
        0x0c, 0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00, 0xcc,
        0xff, 0x00, 0x86, 0xe5, 0xf8, 0xdd, 0xff, 0x00, 0x43, 0xb7, 0xfe, 0x52,
        0x6c, 0x7f, 0xf8, 0xc5, 0x1f, 0xf0, 0xdc, 0xbf, 0x1b, 0xbf, 0xe8, 0x76,
        0xff, 0x00, 0xca, 0x4d, 0x8f, 0xff, 0x00, 0x18, 0xaf, 0x07, 0xa2, 0xbf,
        0x1b, 0xfa, 0xfe, 0x2f, 0xfe, 0x7f, 0x4b, 0xff, 0x00, 0x02, 0x7f, 0xe6,
        0x7f, 0xa5, 0x1f, 0xea, 0x9f, 0x0f, 0x7f, 0xd0, 0xba, 0x8f, 0xfe, 0x0a,
        0x87, 0xff, 0x00, 0x22, 0x6d, 0x7f, 0xc3, 0xc8, 0xbf, 0x68, 0xbf, 0xfa,
        0x28, 0x9f, 0xf9, 0x44, 0xd3, 0xbf, 0xf9, 0x1e, 0x8f, 0xf8, 0x79, 0x17,
        0xed, 0x17, 0xff, 0x00, 0x45, 0x13, 0xff, 0x00, 0x28, 0x9a, 0x77, 0xff,
        0x00, 0x23, 0xd7, 0xcd, 0x14, 0x57, 0xfb, 0x19, 0xfe, 0xa9, 0xf0, 0xf7,
        0xfd, 0x0b, 0xa8, 0xff, 0x00, 0xe0, 0xa8, 0x7f, 0xf2, 0x27, 0xf9, 0x4d,
        0xed, 0xea, 0xff, 0x00, 0x3b, 0xfb, 0xd9, 0xff, 0xd9,
    ];

    /// The same image with an EXIF APP1 segment (hand-assembled to the
    /// EXIF 2.32 layout, an independent source of truth from any encoder)
    /// carrying `Orientation` = value.
    fn jpeg_with_exif_orientation(orientation: u16) -> Vec<u8> {
        let mut exif = Vec::new();
        exif.extend_from_slice(b"Exif\0\0");
        exif.extend_from_slice(&[0x4d, 0x4d, 0x00, 0x2a]); // big-endian TIFF
        exif.extend_from_slice(&[0x00, 0x00, 0x00, 0x08]); // IFD0 at +8
        exif.extend_from_slice(&[0x00, 0x01]); // 1 IFD0 entry
        // IFD0 entry: tag 0x8769 (ExifIFD pointer), LONG, count 1, +0x1a.
        exif.extend_from_slice(&[0x87, 0x69, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1a]);
        exif.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // next IFD: none
        exif.extend_from_slice(&[0x00, 0x01]); // 1 EXIF IFD entry
        // EXIF entry: tag 0x0112 (Orientation), SHORT, count 1, value.
        exif.extend_from_slice(&0x0112u16.to_be_bytes());
        exif.extend_from_slice(&[0x00, 0x03]);
        exif.extend_from_slice(&0x0000_0001u32.to_be_bytes());
        exif.extend_from_slice(&orientation.to_be_bytes());
        exif.extend_from_slice(&[0x00, 0x00]); // high half of the SHORT value field
        exif.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // next IFD: none
        // APP1 segment length counts its own two length bytes.
        let mut segment = vec![0xff, 0xe1];
        segment.extend_from_slice(&((exif.len() as u16 + 2).to_be_bytes()));
        segment.extend_from_slice(&exif);
        // Splice the APP1 right after the existing APP0/JFIF segment.
        let mut out = A3_FIXTURE_JPEG.to_vec();
        assert_eq!(&out[..2], &[0xff, 0xd8], "fixture starts with SOI");
        assert_eq!(out[2], 0xff, "fixture carries an APP0 marker");
        let app0_len = u16::from_be_bytes([out[4], out[5]]) as usize;
        let insert_at = 4 + app0_len; // SOI + APP0 segment
        out.splice(insert_at..insert_at, segment);
        out
    }

    /// PNG bytes masquerading as a JPEG job.
    const A3_FIXTURE_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
        0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00,
        0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    /// The exact host-pushed svc lines (host → guest outbox mirror).
    fn a3_sent(harness: &A3Harness) -> Vec<Value> {
        harness
            .sent
            .iter()
            .map(|line| serde_json::from_str(line).expect("harness lines are JSON"))
            .collect()
    }

    fn a3_write_fixture(name: &str, bytes: &[u8]) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("pocketjs-a3-{}-{name}", std::process::id()));
        std::fs::write(&path, bytes).expect("write fixture");
        path
    }

    #[test]
    fn a3_manifest_is_pushed_once_and_is_bounded_semantic_state() {
        let surface = UiSurface::new((720.0, 480.0));
        let files = vec![PathBuf::from("C:/samples/a.jpg"), PathBuf::from("C:/b.jpg")];
        let mut harness = A3Harness::new(true, files);
        harness.boot(&surface);
        harness.boot(&surface);
        let lines = a3_sent(&harness);
        assert_eq!(lines.len(), 1, "exactly one manifest line, ever");
        assert_eq!(lines[0]["t"], "a3manifest");
        assert_eq!(lines[0]["files"][0], "C:/samples/a.jpg");

        // Inactive harness: no manifest at all.
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A3Harness::new(false, vec![PathBuf::from("C:/a.jpg")]);
        harness.boot(&surface);
        assert!(harness.sent.is_empty());
    }

    #[test]
    fn a3_missing_file_pushes_bounded_error_and_harness_continues() {
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A3Harness::new(true, vec![]);
        harness.boot(&surface);
        let missing = std::env::temp_dir().join("pocketjs-a3-does-not-exist.jpg");
        harness.observe_rx(
            &surface,
            &json!({"t": "a3open", "req": "r1", "path": missing}).to_string(), 0,
        );
        harness.observe_rx(
            &surface,
            &json!({"t": "a3open", "req": "r2", "path": missing}).to_string(), 0,
        );
        harness.process_pending(&surface, 1);
        let lines = a3_sent(&harness);
        assert_eq!(
            lines.len(),
            2,
            "every queued request gets exactly one bounded reply"
        );
        // A5 drain semantics: r1 was superseded by r2 before any work ran,
        // so it is answered with `cancelled`; the newest gets the real
        // outcome and the harness does not wedge on failure.
        assert_eq!(lines[0]["t"], "a3error");
        assert_eq!(lines[0]["req"], "r1");
        assert_eq!(lines[0]["code"], "cancelled");
        assert_eq!(lines[1]["req"], "r2");
        assert_eq!(lines[1]["code"], "missing");
        assert_eq!(harness.failures, 2);
        assert_eq!(harness.successes, 0);
        let live = surface.with_ui(|ui| ui.texture_live_bytes());
        assert_eq!(live, 0, "no resource may exist for a failed request");
    }

    #[test]
    fn exif_orientation_transform_matches_the_exif_table() {
        // (rotate clockwise, then flip horizontal) must equal the EXIF
        // orientation semantics for every defined value. Derived from the
        // EXIF 2.32 camera-position table by matrix composition: 2 = mirror
        // horizontal; 3 = 180; 4 = flipV = flipH∘R180; 5 = transpose
        // (flipH∘R90); 6 = R90; 7 = transverse (flipH∘R270); 8 = R270.
        let table: [(u16, QuarterTurn, bool); 8] = [
            (1, QuarterTurn::R0, false),
            (2, QuarterTurn::R0, true),
            (3, QuarterTurn::R180, false),
            (4, QuarterTurn::R180, true),
            (5, QuarterTurn::R90, true),
            (6, QuarterTurn::R90, false),
            (7, QuarterTurn::R270, true),
            (8, QuarterTurn::R270, false),
        ];
        for (value, rotate, flip) in table {
            let t = exif_orientation_transform(value);
            assert_eq!((t.rotate, t.flip_h), (rotate, flip), "EXIF {value}");
        }
        // Undefined values fall back to orientation 1, never to an error.
        for value in [0u16, 9, 42, u16::MAX] {
            let t = exif_orientation_transform(value);
            assert_eq!((t.rotate, t.flip_h), (QuarterTurn::R0, false));
        }
    }

    #[test]
    fn a3_decode_plane_admission_is_checked_before_allocation() {
        // The core admission bound (NATIVE_TEX_MAX_DIM) and byte-length
        // overflow are rejected before any pixel plane is allocated.
        assert_eq!(checked_plane_bytes(3840, 2160).unwrap(), 3840 * 2160 * 4);
        assert_eq!(checked_plane_bytes(1, 1).unwrap(), 4);
        assert!(matches!(
            checked_plane_bytes(0, 100),
            Err(A3DecodeError::Corrupt)
        ));
        assert!(matches!(
            checked_plane_bytes(8193, 10),
            Err(A3DecodeError::TooLarge)
        ));
        assert!(matches!(
            checked_plane_bytes(10, 8193),
            Err(A3DecodeError::TooLarge)
        ));
        assert!(matches!(
            checked_plane_bytes(u32::MAX, u32::MAX),
            Err(A3DecodeError::TooLarge)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn a3_wic_decodes_jpeg_fixture_to_rgba_with_orientation_metadata() {
        let decoded = decode_jpeg_wic(A3_FIXTURE_JPEG).expect("fixture decodes");
        assert_eq!(decoded.w, 32);
        assert_eq!(decoded.h, 16);
        assert_eq!(decoded.orientation, 1, "no EXIF tag means orientation 1");
        assert_eq!(decoded.pixels.len(), 32 * 16 * 4);
        assert!(decoded.pixels.iter().skip(3).step_by(4).all(|&a| a == 255));
        let at = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * decoded.w + x) * 4) as usize;
            [
                decoded.pixels[i],
                decoded.pixels[i + 1],
                decoded.pixels[i + 2],
                decoded.pixels[i + 3],
            ]
        };
        let left = at(4, 8);
        assert!(
            left[0] > 140 && left[0] > left[1] + 40 && left[0] > left[2] + 40,
            "left half must be red-ish, got {left:?}"
        );
        let right = at(27, 8);
        assert!(
            right[2] > 140 && right[2] > right[0] + 40 && right[2] > right[1] + 40,
            "right half must be blue-ish, got {right:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a3_wic_applies_exif_orientation_natively() {
        let bytes = jpeg_with_exif_orientation(6);
        let decoded = decode_jpeg_wic(&bytes).expect("fixture decodes");
        assert_eq!(decoded.orientation, 6);
        // Stored 32x16 with orientation 6 displays as 16x32 (rotate 90 CW).
        assert_eq!(decoded.w, 16);
        assert_eq!(decoded.h, 32);
        let at = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * decoded.w + x) * 4) as usize;
            [
                decoded.pixels[i],
                decoded.pixels[i + 1],
                decoded.pixels[i + 2],
                decoded.pixels[i + 3],
            ]
        };
        // The stored left (red) half becomes the displayed TOP half; the
        // stored right (blue) half becomes the displayed BOTTOM half.
        let top = at(8, 2);
        assert!(
            top[0] > 140 && top[0] > top[1] + 40 && top[0] > top[2] + 40,
            "displayed top half must be red-ish, got {top:?}"
        );
        let bottom = at(8, 29);
        assert!(
            bottom[2] > 140 && bottom[2] > bottom[0] + 40 && bottom[2] > bottom[1] + 40,
            "displayed bottom half must be blue-ish, got {bottom:?}"
        );
    }

    /// Decode an oriented fixture and sample four diagnostic points.
    /// The fixture is 32x16 (left red / right blue / white top rows), so
    /// after the 90°-family transforms the displayed 16x32 image has the
    /// red/blue halves on top/bottom and the white stored rows on the
    /// left or right columns — the side distinguishes transpose-family
    /// orientations from their rotate-only siblings (the pixel oracle
    /// cannot be satisfied by the wrong quarter-turn + flip).
    #[cfg(windows)]
    fn oriented_quadrant_samples(orientation: u16) -> ([u8; 4], [u8; 4], [u8; 4], [u8; 4]) {
        let decoded = decode_jpeg_wic(&jpeg_with_exif_orientation(orientation))
            .expect("fixture decodes");
        assert_eq!(decoded.orientation, orientation);
        assert_eq!((decoded.w, decoded.h), (16, 32), "stored 32x16 rotated 90");
        let at = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * decoded.w + x) * 4) as usize;
            [
                decoded.pixels[i],
                decoded.pixels[i + 1],
                decoded.pixels[i + 2],
                decoded.pixels[i + 3],
            ]
        };
        (at(8, 2), at(8, 29), at(1, 16), at(14, 16))
    }

    #[cfg(windows)]
    fn assert_red(p: [u8; 4], what: &str) {
        assert!(p[0] > 140 && p[0] > p[1] + 40 && p[0] > p[2] + 40, "{what} must be red-ish, got {p:?}");
    }

    #[cfg(windows)]
    fn assert_blue(p: [u8; 4], what: &str) {
        assert!(p[2] > 140 && p[2] > p[0] + 40 && p[2] > p[1] + 40, "{what} must be blue-ish, got {p:?}");
    }

    #[cfg(windows)]
    fn assert_white(p: [u8; 4], what: &str) {
        assert!(p[0] > 190 && p[1] > 190 && p[2] > 190, "{what} must be white-ish, got {p:?}");
    }

    #[cfg(windows)]
    #[test]
    fn a3_wic_orients_exif_5_as_transpose() {
        // Transpose = flipH∘R90: stored red half → TOP, stored white rows
        // → displayed LEFT columns.
        let (top, bottom, left_col, right_col) = oriented_quadrant_samples(5);
        assert_red(top, "orient-5 top");
        assert_blue(bottom, "orient-5 bottom");
        assert_white(left_col, "orient-5 left column");
        assert_blue(right_col, "orient-5 right column");
    }

    #[cfg(windows)]
    #[test]
    fn a3_wic_orients_exif_7_as_transverse() {
        // Transverse = flipH∘R270: (x,y) → (15-y, 31-x). Stored red half →
        // BOTTOM, stored white rows → displayed RIGHT columns. Left-column
        // sample (0,16) ← stored (15,15) = red side.
        let (top, bottom, left_col, right_col) = oriented_quadrant_samples(7);
        assert_blue(top, "orient-7 top");
        assert_red(bottom, "orient-7 bottom");
        assert_red(left_col, "orient-7 left column");
        assert_white(right_col, "orient-7 right column");
    }

    // ------------------------------------------------------------------
    // A4 scaled-decode probes (WIC inbox JPEG source-transform path).
    // The 32x16 fixture supports exact DCT scales 1/1, 1/2, 1/4, 1/8:
    // 32x16, 16x8, 8x4, 4x2.
    // ------------------------------------------------------------------

    #[cfg(windows)]
    #[test]
    fn a4_source_transform_reports_closest_native_size() {
        // Exact native scale: requested 8x4 is exactly the 1/4 DCT scale.
        let probe = probe_source_transform(&A3_FIXTURE_JPEG, 8, 4).expect("probe");
        assert!(probe.supported, "inbox JPEG decoder must expose the source-transform path");
        assert_eq!((probe.native_w, probe.native_h), (8, 4));

        // Requests larger than the source never upscale.
        let probe = probe_source_transform(&A3_FIXTURE_JPEG, 64, 32).expect("probe");
        assert_eq!((probe.native_w, probe.native_h), (32, 16));

        // Non-power-of-two requests snap to a supported DCT scale — the
        // measured choice is recorded, only membership is asserted here so
        // the test does not encode this machine's tie-breaking.
        let probe = probe_source_transform(&A3_FIXTURE_JPEG, 7, 3).expect("probe");
        assert!(
            [(8, 4), (4, 2)].contains(&(probe.native_w, probe.native_h)),
            "closest size must snap to a DCT scale, got {}x{}",
            probe.native_w,
            probe.native_h
        );
    }

    #[cfg(windows)]
    #[test]
    fn a4_scaled_decode_produces_oriented_plane_at_native_size() {
        // Orientation 6: stored 32x16 → displayed 16x32; DCT 1/4 gives
        // stored 8x4 → displayed 4x8. The request is expressed in DISPLAY
        // space and inverted internally before the stored-space query.
        let out = decode_jpeg_wic_scaled(&jpeg_with_exif_orientation(6), 4, 8)
            .expect("scaled decode");
        assert_eq!(out.decoded.orientation, 6);
        assert!(out.via_source_transform, "must use the decoder's own scaling");
        assert_eq!((out.native_w, out.native_h), (8, 4), "closest stored size");
        assert_eq!((out.decoded.w, out.decoded.h), (4, 8), "oriented display size");
        assert_eq!(out.decoded.pixels.len(), 4 * 8 * 4);
        assert!(out.decoded.pixels.iter().skip(3).step_by(4).all(|&a| a == 255));
    }

    #[cfg(windows)]
    #[test]
    fn a3_wic_rejects_corrupt_and_mislabeled_content_with_bounded_codes() {
        // Non-JPEG content is refused before any decoder runs.
        assert!(matches!(
            decode_jpeg_wic(A3_FIXTURE_PNG),
            Err(A3DecodeError::NotJpeg)
        ));
        // A JPEG header truncated mid-table cannot produce pixels.
        let truncated = &A3_FIXTURE_JPEG[..200];
        assert!(matches!(
            decode_jpeg_wic(truncated),
            Err(A3DecodeError::Corrupt)
        ));
        // Absurd-but-declared dimensions are rejected before allocation:
        // 9000 per axis is inside WIC's own limit (65500) but above the
        // native admission bound (8192), so OUR check must fire first.
        let huge = a3_huge_fixture(9000, 9000);
        assert!(matches!(
            decode_jpeg_wic(&huge),
            Err(A3DecodeError::TooLarge)
        ));
    }

    /// The plain fixture with its SOF0 dimensions patched to the given
    /// (illegal-for-admission) values: header parses, pixels cannot exist.
    fn a3_huge_fixture(w: u16, h: u16) -> Vec<u8> {
        let mut huge = A3_FIXTURE_JPEG.to_vec();
        let sof = huge
            .windows(2)
            .position(|w| w == [0xff, 0xc0])
            .expect("fixture has SOF0");
        huge[sof + 5] = (h >> 8) as u8;
        huge[sof + 6] = (h & 0xff) as u8;
        huge[sof + 7] = (w >> 8) as u8;
        huge[sof + 8] = (w & 0xff) as u8;
        huge
    }

    #[cfg(windows)]
    #[test]
    fn a4_fit_request_decodes_scaled_and_announces_native_size() {
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A3Harness::new(true, vec![]);
        harness.boot(&surface);

        let path = a3_write_fixture("fit.jpg", A3_FIXTURE_JPEG);
        harness.observe_rx(
            &surface,
            &json!({"t": "a4open", "req": "f1", "path": path, "fitW": 8, "fitH": 4})
                .to_string(),
            0,
        );
        harness.process_pending(&surface, 1);
        let lines = a3_sent(&harness);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["t"], "a3img");
        assert_eq!(lines[0]["req"], "f1");
        assert_eq!(lines[0]["mode"], "fit");
        assert_eq!(lines[0]["nativeW"], 8);
        assert_eq!(lines[0]["nativeH"], 4);
        assert_eq!(lines[0]["w"], 8);
        assert_eq!(lines[0]["h"], 4);
        assert_eq!(surface.with_ui(|ui| ui.texture_live_bytes()), 8 * 4 * 4);
        std::fs::remove_file(&path).ok();
    }

    #[cfg(windows)]
    #[test]
    fn a4_full_request_on_oversized_source_degrades_explicitly() {
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A3Harness::new(true, vec![]);
        harness.boot(&surface);

        let path = a3_write_fixture("huge.jpg", &a3_huge_fixture(9000, 9000));
        // A 100% (no fit dims) request on an over-admission source must
        // produce a bounded, explicit degrade — never an allocation.
        harness.observe_rx(
            &surface,
            &json!({"t": "a4open", "req": "z1", "path": path}).to_string(),
            0,
        );
        harness.process_pending(&surface, 1);
        let lines = a3_sent(&harness);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["t"], "a3error");
        assert_eq!(lines[0]["req"], "z1");
        assert_eq!(lines[0]["code"], "too_large");
        assert_eq!(surface.with_ui(|ui| ui.texture_live_bytes()), 0);
        std::fs::remove_file(&path).ok();
    }

    #[cfg(windows)]
    #[test]
    fn a3_open_request_decodes_registers_and_retires_previous_synchronously() {
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A3Harness::new(true, vec![]);
        harness.boot(&surface);

        let path = a3_write_fixture("flow.jpg", A3_FIXTURE_JPEG);
        harness.observe_rx(
            &surface,
            &json!({"t": "a3open", "req": "r1", "path": path}).to_string(), 0,
        );
        harness.process_pending(&surface, 1);
        let lines = a3_sent(&harness);
        assert_eq!(lines.len(), 1, "one bounded announcement per request");
        assert_eq!(lines[0]["t"], "a3img");
        assert_eq!(lines[0]["req"], "r1");
        assert_eq!(lines[0]["w"], 32);
        assert_eq!(lines[0]["h"], 16);
        assert_eq!(lines[0]["orient"], 1);
        let handle_1 = lines[0]["handle"].as_i64().unwrap();
        assert!(handle_1 >= 0);
        let plane = 32 * 16 * 4;
        assert_eq!(surface.with_ui(|ui| ui.texture_live_bytes()), plane);
        assert!(surface.with_ui(|ui| ui.texture(handle_1 as i32).is_some()));
        assert_eq!(harness.successes, 1);

        // A second open publishes through the same seam and synchronously
        // retires the previous resource: exactly one plane stays live and
        // the stale handle resolves to absence without any GC step.
        harness.observe_rx(
            &surface,
            &json!({"t": "a3open", "req": "r2", "path": path}).to_string(), 0,
        );
        harness.process_pending(&surface, 2);
        let lines = a3_sent(&harness);
        assert_eq!(lines.len(), 2, "one bounded announcement per request");
        assert_eq!(lines[1]["t"], "a3img");
        assert_eq!(lines[1]["req"], "r2");
        let handle_2 = lines[1]["handle"].as_i64().unwrap();
        assert_ne!(handle_1, handle_2, "handles are generation-tagged");
        assert_eq!(surface.with_ui(|ui| ui.texture_live_bytes()), plane);
        assert!(surface.with_ui(|ui| ui.texture(handle_1 as i32).is_none()));
        assert!(surface.with_ui(|ui| ui.texture(handle_2 as i32).is_some()));
        assert_eq!(harness.successes, 2);
        assert_eq!(harness.failures, 0);

        // Every svc line on this path is bounded semantic state: no line
        // may approach pixel size (the plane itself is 2048 bytes, the
        // largest allowed line is the manifest — asserted far below it).
        let total_tx = harness.tx_bytes;
        assert!(total_tx < 4096, "svc traffic must be bounded, got {total_tx}");
        std::fs::remove_file(&path).ok();
    }

    #[cfg(windows)]
    #[test]
    fn a5_rapid_requests_coalesce_and_only_the_newest_publishes() {
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A3Harness::new(true, vec![]);
        harness.boot(&surface);

        let path = a3_write_fixture("stress.jpg", A3_FIXTURE_JPEG);
        // 120 rapid requests through the real guest→host request seam: no
        // decode may start inside the drain, every superseded request gets
        // one bounded `cancelled` reply BEFORE any decode stage, and only
        // the newest requested generation may decode and publish.
        for i in 0..120 {
            harness.observe_rx(
                &surface,
                &json!({"t": "a4open", "req": format!("s{i}"), "path": path,
                        "fitW": 16, "fitH": 8}).to_string(),
                i as u64,
            );
        }
        assert!(a3_sent(&harness).is_empty(), "no work may start inside the drain");
        harness.process_pending(&surface, 120);

        let lines = a3_sent(&harness);
        let cancels: Vec<&Value> =
            lines.iter().filter(|l| l["code"] == "cancelled").collect();
        let imgs: Vec<&Value> = lines.iter().filter(|l| l["t"] == "a3img").collect();
        assert_eq!(cancels.len(), 119, "one bounded cancel per superseded request");
        assert_eq!(imgs.len(), 1, "exactly one decode/publish for the whole burst");
        assert_eq!(
            imgs[0]["req"], "s119",
            "the newest requested generation is the one published"
        );
        assert_eq!(harness.successes, 1);
        let plane = 16 * 8 * 4;
        assert_eq!(
            surface.with_ui(|ui| ui.texture_live_bytes()),
            plane,
            "obsolete resources are already retired; exactly one plane lives"
        );
        // 120 requests produced bounded traffic, never pixel-sized lines.
        assert!(harness.tx_bytes < 16 * 1024);
        std::fs::remove_file(&path).ok();
    }

    #[cfg(windows)]
    #[test]
    fn a5_interleaved_generations_publish_strictly_in_request_order() {
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A3Harness::new(true, vec![]);
        harness.boot(&surface);
        let path = a3_write_fixture("gen.jpg", A3_FIXTURE_JPEG);

        let mut published: Vec<String> = Vec::new();
        let mut tick = 0u64;
        // Three waves of 10 generations each. Each wave is drained once:
        // the publish sequence must be exactly the newest generation of
        // each wave, in wave order — an older generation can never publish
        // over a newer one, and a newer one never skips an older wave's
        // settled publish.
        for wave in 0..3 {
            for k in 0..10 {
                harness.observe_rx(
                    &surface,
                    &json!({"t": "a4open", "req": format!("w{wave}k{k}"), "path": path,
                            "fitW": 16, "fitH": 8}).to_string(),
                    tick,
                );
                tick += 1;
            }
            let before = a3_sent(&harness).len();
            harness.process_pending(&surface, tick);
            let fresh = &a3_sent(&harness)[before..];
            published.extend(
                fresh.iter()
                    .filter(|l| l["t"] == "a3img")
                    .map(|l| l["req"].as_str().unwrap().to_string()),
            );
        }
        assert_eq!(
            published,
            vec!["w0k9".to_string(), "w1k9".to_string(), "w2k9".to_string()],
            "publish order is strictly newest-per-drain, never stale"
        );
        std::fs::remove_file(&path).ok();
    }

    #[cfg(windows)]
    #[test]
    fn a5_hostile_batch_fails_bounded_and_recovers_without_crash() {
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A3Harness::new(true, vec![]);
        harness.boot(&surface);

        // Each hostile input runs as the newest of its own drain so its
        // real failure stage executes (a superseded input would be
        // cancelled before decode — that path is covered by the coalesce
        // test above).
        let truncated = a3_write_fixture("trunc.jpg", &A3_FIXTURE_JPEG[..200]);
        let fake = a3_write_fixture("fake.jpg", A3_FIXTURE_PNG);
        let huge = a3_write_fixture("huge.jpg", &a3_huge_fixture(9000, 9000));
        let absurd = a3_write_fixture("absurd.jpg", &a3_huge_fixture(u16::MAX, u16::MAX));
        let good = a3_write_fixture("good.jpg", A3_FIXTURE_JPEG);
        let missing = std::env::temp_dir().join("pocketjs-a5-does-not-exist.jpg");

        let hostile: [(&str, PathBuf, &str); 5] = [
            ("h0", missing, "missing"),
            ("h1", fake, "not_jpeg"),
            ("h2", truncated, "corrupt"),
            ("h3", huge, "too_large"),
            // 65535 per axis: above our admission bound; WIC's own decoder
            // limit may also refuse it — either way the code is bounded.
            ("h4", absurd, "too_large_or_corrupt"),
        ];
        for (i, (req, path, _)) in hostile.iter().enumerate() {
            harness.observe_rx(
                &surface,
                &json!({"t": "a4open", "req": req, "path": path}).to_string(),
                i as u64,
            );
            harness.process_pending(&surface, i as u64 + 1);
        }
        for (req, _, expected) in &hostile {
            let hit = a3_sent(&harness).iter().rev().find(|l| &l["req"] == req)
                .cloned()
                .expect("each hostile request gets exactly one reply");
            assert_eq!(hit["t"], "a3error");
            if *expected == "too_large_or_corrupt" {
                // The absurd-dimension header is rejected either by WIC's
                // own decoder limit (corrupt) or by our admission check
                // (too_large); both are bounded pre-allocation outcomes.
                assert!(
                    hit["code"] == "too_large" || hit["code"] == "corrupt",
                    "absurd dims must degrade bounded, got {}",
                    hit["code"]
                );
            } else {
                assert_eq!(hit["code"], *expected);
            }
        }
        assert_eq!(harness.successes, 0, "nothing hostile may publish");

        // The process keeps serving: a valid request right after the
        // hostile batch decodes, publishes, and owns exactly one plane.
        harness.observe_rx(
            &surface,
            &json!({"t": "a4open", "req": "h5", "path": good, "fitW": 16, "fitH": 8})
                .to_string(),
            10,
        );
        harness.process_pending(&surface, 11);
        let lines_after = a3_sent(&harness);
        let imgs: Vec<&Value> =
            lines_after.iter().filter(|l| l["t"] == "a3img").collect();
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0]["req"], "h5");
        assert_eq!(
            surface.with_ui(|ui| ui.texture_live_bytes()),
            16 * 8 * 4,
            "exactly the newest valid plane is live after hostile inputs"
        );
        std::fs::remove_file(&good).ok();
        for (_, path, _) in hostile {
            std::fs::remove_file(path).ok();
        }
    }

    #[cfg(windows)]
    #[test]
    fn a5_cancellations_never_disturb_the_live_resource_or_boundary_budget() {
        let surface = UiSurface::new((720.0, 480.0));
        let mut harness = A3Harness::new(true, vec![]);
        harness.boot(&surface);
        let path = a3_write_fixture("live.jpg", A3_FIXTURE_JPEG);

        // Settle one published image.
        harness.observe_rx(
            &surface,
            &json!({"t": "a4open", "req": "r1", "path": path, "fitW": 16, "fitH": 8})
                .to_string(),
            0,
        );
        harness.process_pending(&surface, 1);

        // A cancellation burst must leave the live resource untouched and
        // the boundary traffic bounded (no pixel-sized line, no churn).
        let tx_before = harness.tx_bytes;
        for i in 0..50 {
            harness.observe_rx(
                &surface,
                &json!({"t": "a4open", "req": format!("c{i}"), "path": path,
                        "fitW": 16, "fitH": 8}).to_string(),
                i as u64 + 2,
            );
        }
        harness.process_pending(&surface, 52);
        // Newest of the burst (c49) replaces r1; c0..c48 were cancelled.
        assert_eq!(surface.with_ui(|ui| ui.texture_live_bytes()), 16 * 8 * 4);
        assert_eq!(harness.successes, 2, "r1 + the newest of the burst only");
        assert_eq!(harness.cancels, 49);
        let per_request = (harness.tx_bytes - tx_before) / 50;
        assert!(per_request < 512, "cancel replies are bounded, got {per_request} B/req");
        std::fs::remove_file(&path).ok();
    }

    // ------------------------------------------------------------------
    // A6: Per-Monitor DPI V2 coordinate domains. Image coordinates live in
    // texture space; the guest composes in logical viewport coordinates;
    // the window reports physical client pixels; the monitor DPI is
    // 96 × scale. The raster density is what couples logical to physical
    // at present time, and it must FOLLOW the scale instead of freezing
    // at the plan value.

    #[test]
    fn a6_logical_viewport_is_stable_across_scale_changes() {
        // Policy: the logical viewport is the invariant; physical client
        // pixels are logical × scale. Converting back must return the
        // identical logical viewport at every scale step — no stale
        // prior-monitor physical transform may survive.
        let logical = (720u32, 480u32);
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let physical = (
                (logical.0 as f64 * scale).round() as u32,
                (logical.1 as f64 * scale).round() as u32,
            );
            assert_eq!(
                logical_from_physical(physical.0, physical.1, scale),
                logical,
                "round trip at scale {scale}"
            );
        }
        // The clamps this conversion has always applied are preserved.
        assert_eq!(logical_from_physical(20000, 20000, 1.0), (4096, 4096));
        assert_eq!(logical_from_physical(0, 0, 1.0).0.max(logical_from_physical(0, 0, 1.0).1) >= 240, true);
    }

    #[test]
    fn a6_effective_density_follows_window_scale_when_driven() {
        // Without a driven scale the plan density stays authoritative
        // (backwards compatible with every earlier ticket's runs).
        assert_eq!(effective_density(2, None), 2);
        assert_eq!(effective_density(1, None), 1);
        // Once a scale transition is driven, the raster tracks the
        // window scale: 100% -> 1x, 200% -> 2x, fractional scales round
        // to the nearest integer sample density.
        assert_eq!(effective_density(2, Some(1.0)), 1);
        assert_eq!(effective_density(2, Some(2.0)), 2);
        assert_eq!(effective_density(1, Some(2.0)), 2, "plan density is a floor for driven scales above it");
        assert_eq!(effective_density(2, Some(1.5)), 2);
        assert_eq!(effective_density(2, Some(1.25)), 1);
        assert_eq!(effective_density(2, Some(0.5)), 1, "never below 1x");
        assert_eq!(effective_density(2, Some(6.0)), 4, "clamped like --density");
    }
}
