const cmanifest = Services.dirsvc.get("UChrm", Ci.nsIFile);
cmanifest.append("chrome.manifest");
if (cmanifest.exists()) {
  Components.manager.QueryInterface(Ci.nsIComponentRegistrar).autoRegister(cmanifest);
  ChromeUtils.importESModule("chrome://userchrome/content/bootstrap.sys.mjs");
}

