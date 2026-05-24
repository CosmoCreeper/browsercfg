if (Services.prefs.getBoolPref("browsercfg.test.active", false)) {
    const port = Services.prefs.getIntPref("browsercfg.test.port");
    const ws = new WebSocket(`ws://127.0.0.1:${port}`);

    ws.addEventListener("open", async () => {
        const testDir = Services.dirsvc.get("UChrm", Ci.nsIFile);
        testDir.append("tests");

        const entries = testDir.directoryEntries;
        while (entries.hasMoreElements()) {
            const file = entries.getNext().QueryInterface(Ci.nsIFile);
            if (!file.leafName.endsWith(".test.js")) continue;

            const fileURI = Services.io.newFileURI(file).spec;
            const tests = [];

            // Minimal it logic, implement more logic in the future
            // DOM support with auto-loading into special DOMs should be implemented
            const it = (name, fn) => tests.push({ name, fn });

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
