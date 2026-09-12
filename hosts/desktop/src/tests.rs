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
}
