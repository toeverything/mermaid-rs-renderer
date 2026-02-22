import { rmSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const packageRoot = resolve(fileURLToPath(new URL('..', import.meta.url)))
for (const filename of ['package.json', '.gitignore', 'README.md', 'LICENSE']) {
	const generatedFile = resolve(packageRoot, 'pkg', filename)
	rmSync(generatedFile, { force: true })
}
