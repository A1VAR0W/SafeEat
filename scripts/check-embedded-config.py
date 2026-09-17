"""Verify the configuration actually embedded in the installable archive, not just CI variables."""
import json
import os
import pathlib
import sys
import zipfile

archive = pathlib.Path(sys.argv[1])
with zipfile.ZipFile(archive) as package:
    ios = archive.suffix == ".ipa"
    config_path = "Payload/SafeEat.app/EXConstants.bundle/app.config" if ios else "assets/app.config"
    config = json.loads(package.read(config_path))
    extra = config["extra"]
    assert extra["environment"] == os.environ["APP_ENV"], "Embedded environment mismatch"
    assert extra["apiUrl"] == os.environ["API_PUBLIC_URL"], "Embedded API URL mismatch"
    assert extra["publicUrl"] == os.environ["PUBLIC_APP_URL"], "Embedded public URL mismatch"
    bundle_path = "Payload/SafeEat.app/main.jsbundle" if ios else "assets/index.android.bundle"
    assert package.getinfo(bundle_path).file_size > 100000, "Standalone JS/Hermes bundle missing"
print("Verified embedded release configuration and standalone bundle:", extra["environment"])
