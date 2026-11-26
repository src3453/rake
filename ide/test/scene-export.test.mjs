import {expect, test} from "bun:test";
import {formatScene} from "../gui/overlay/src/lib/rake/scene-export.js";

test("scene export preserves the selected runtime", () => {
    const text = formatScene({
        name: "Main",
        runtime: "turbowarp",
        stage: {backdrops: [], currentBackdrop: 0, sounds: [], scripts: [], volume: 100, tempo: 60},
        sprites: []
    });

    expect(text).toContain('runtime: "turbowarp"');
});

test("scene export omits the default Scratch runtime", () => {
    const text = formatScene({
        name: "Main",
        runtime: "scratch",
        stage: {backdrops: [], currentBackdrop: 0, sounds: [], scripts: [], volume: 100, tempo: 60},
        sprites: []
    });

    expect(text).not.toContain("runtime:");
});
