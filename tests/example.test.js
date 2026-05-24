// tests/prefs.test.js

it("Services is available", () => {
    if (!Services) throw new Error("Services is not defined");
});

it("can read a pref", () => {
    const val = Services.prefs.getBoolPref("browsercfg.test.active", false);
    if (val !== true) throw new Error(`expected true, got ${val}`);
});

it("ChromeUtils is available", () => {
    if (!ChromeUtils) throw new Error("ChromeUtils is not defined");
});

