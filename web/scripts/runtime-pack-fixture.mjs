// Small real producer fixture shared by Node sync tests and Vitest transport tests.
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function runtimePackFixture(out) {
  const root = fileURLToPath(new URL('../../', import.meta.url));
  execFileSync('python3', ['-c', `
import importlib.util
from pathlib import Path
import sys
sys.path.insert(0, str(Path(sys.argv[1]) / 'pipeline'))
spec = importlib.util.spec_from_file_location('fixture', Path(sys.argv[1]) / 'pipeline/test-runtime-pack.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
case = module.RuntimePackTests()
case.setUp()
try:
    case.create(Path(sys.argv[2]).resolve())
finally:
    case.doCleanups()
`, root, resolve(out)]);
}
