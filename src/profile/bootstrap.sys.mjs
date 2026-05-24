if (Services.prefs.getBoolPref("browsercfg.test.active", false)) {
    const port = Services.prefs.getIntPref("browsercfg.test.port");
    const ws = new WebSocket(`ws://127.0.0.1:${port}`);

    ws.addEventListener("open", async () => {
        // Discover and load test files from the profile
        const testDir = Services.dirsvc.get("ProfD", Ci.nsIFile);
        testDir.append("chrome");
        testDir.append("tests");

        const entries = testDir.directoryEntries;
        while (entries.hasMoreElements()) {
            const file = entries.getNext().QueryInterface(Ci.nsIFile);
            if (!file.leafName.endsWith(".test.js")) continue;

            // Load the test file in privileged context
            const fileURI = Services.io.newFileURI(file).spec;
            const tests = [];
            // Expose a minimal describe/it API for test files to register against
            const it = (name, fn) => tests.push({ name, fn });

            // Execute the test file
            Services.scriptloader.loadSubScript(fileURI, { it });

            for (const { name, fn } of tests) {
                try {
                    await fn();
                    ws.send(JSON.stringify({ type: "test:result", status: "pass", name }));
                } catch (err) {
                    ws.send(JSON.stringify({ type: "test:result", status: "fail", name, error: err.message }));
                }
            }
        }

        ws.send(JSON.stringify({ type: "suite:done" }));
        ws.close();
    });
}
