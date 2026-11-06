import { readFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export function checkVersions(versions, tag) {
  const version = versions.package;
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) throw new Error('Invalid application version');
  for (const [source, value] of Object.entries(versions)) {
    if (value !== version) throw new Error(`Version mismatch in ${source}`);
  }
  if (tag !== undefined && tag !== `v${version}`) throw new Error('Release tag does not match application version');
  return version;
}
export function workspaceVersions(root = new URL('../',import.meta.url)) {
  const json = path => JSON.parse(readFileSync(new URL(path,root),'utf8'));
  const cargo = readFileSync(new URL('src-tauri/Cargo.toml',root),'utf8');
  const block = cargo.match(/(?:^|\n)\[package\]\s*\r?\n([\s\S]*?)(?=\r?\n\[|$)/)?.[1];
  return {package:json('package.json').version,tauri:json('src-tauri/tauri.conf.json').version,
    cargo:block?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1]};
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const version = checkVersions(workspaceVersions(), process.env.RELEASE_TAG);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT,`version=${version}\n`);
  console.log(`Application versions agree: ${version}`);
}
