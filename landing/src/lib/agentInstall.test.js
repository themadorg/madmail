import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { agentInstallCommand, agentQuickstart } from './agentInstall.js';

// Exercise the published shell line without networking, sudo, or host changes.
function runRecipe(failStage = '') {
	const directory = mkdtempSync(join(tmpdir(), 'madmail-agent-install-'));
	try {
		const bin = join(directory, 'bin');
		mkdirSync(bin);
		const executable = (path, source) => writeFileSync(path, `#!/bin/sh\nset -eu\n${source}\n`, { mode: 0o755 });
		executable(join(directory, 'fixture-mail'), `printf '%s\\n' "$*" >> "$CALL_LOG"
case "$*" in
  install*) stage=install ;;
  *websmtp*) stage=websmtp ;;
  *webimap*) stage=webimap ;;
  *) exit 99 ;;
esac
[ "$FAIL_STAGE" != "$stage" ]`);
		executable(join(bin, 'curl'), `printf '%s\\n' "download $*" >> "$CALL_LOG"
[ "$FAIL_STAGE" != download ]
cp "$FIXTURE_MAIL" madmail`);
		executable(join(bin, 'sudo'), `case "$1" in
  /usr/local/bin/madmail) shift; exec ./madmail "$@" ;;
  ./madmail|systemctl) exec "$@" ;;
  *) exit 99 ;;
esac`);
		executable(join(bin, 'systemctl'), `printf '%s\\n' "service $*" >> "$CALL_LOG"`);
		const result = spawnSync('/bin/sh', ['-c', agentInstallCommand], {
			cwd: directory,
			encoding: 'utf8',
			env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, FAIL_STAGE: failStage,
				CALL_LOG: join(directory, 'calls'), FIXTURE_MAIL: join(directory, 'fixture-mail') }
		});
		return { status: result.status, calls: readFileSync(join(directory, 'calls'), 'utf8').trim().split('\n') };
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
}

test('the guide and landing disclose the same one-line installation recipe', () => {
	const guide = readFileSync(new URL('../../../docs/project/user-guide/19-ai-agent-email.md', import.meta.url), 'utf8');
	assert.ok(guide.includes(`\n${agentInstallCommand}\n`));
	assert.ok(!agentInstallCommand.includes('\n'));
	assert.equal(agentQuickstart, '/docs/project/user-guide/19-ai-agent-email');
});

test('install recipe applies both mail API settings before starting the service', () => {
	const { status, calls } = runRecipe();
	assert.equal(status, 0);
	assert.equal(calls.length, 5);
	assert.match(calls[0], /^download .*https:\/\/github.com\/themadorg\/madmail\/releases\/latest\/download\/madmail-linux-amd64/);
	assert.equal(calls[1], 'install --simple --domain agents.example.org --acme-email admin@example.org --lang en');
	assert.equal(calls[2], '--config /etc/madmail/madmail.conf --state-dir /var/lib/madmail websmtp enable');
	assert.equal(calls[3], '--config /etc/madmail/madmail.conf --state-dir /var/lib/madmail webimap enable');
	assert.equal(calls[4], 'service enable --now madmail');
});

for (const [stage, count] of [['download', 1], ['install', 2], ['websmtp', 3], ['webimap', 4]]) {
	test(`failed ${stage} stops before further configuration or service startup`, () => {
		const { status, calls } = runRecipe(stage);
		assert.notEqual(status, 0);
		assert.equal(calls.length, count);
		assert.ok(calls.every((call) => !call.startsWith('service ')));
	});
}
