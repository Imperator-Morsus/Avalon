const { spawn, execSync } = require('child_process');

// Test the exact stopBackend logic from main.js

let backendProcess = null;

function startDummyBackend() {
  // Use a long-running Windows command that we can identify
  backendProcess = spawn('ping', ['127.0.0.1', '-t'], {
    detached: false,
    windowsHide: true,
  });
  console.log('Started dummy backend PID:', backendProcess.pid);
}

function stopBackend() {
  if (!backendProcess) return;
  const pid = backendProcess.pid;

  if (process.platform === 'win32') {
    try {
      execSync(`taskkill /PID ${pid} /T /F`, { stdio: 'ignore' });
      console.log('taskkill succeeded for PID', pid);
    } catch (e) {
      console.log('taskkill failed (process may already be dead):', e.message);
    }
  } else {
    try {
      process.kill(pid, 'SIGTERM');
    } catch (e) {
      console.log('kill failed:', e.message);
    }
  }

  backendProcess = null;
}

function isRunning(pid) {
  try {
    // Windows-specific check: tasklist and grep for PID
    execSync(`tasklist /FI "PID eq ${pid}" | findstr "${pid}"`, { stdio: 'ignore' });
    return true;
  } catch (e) {
    return false;
  }
}

console.log('=== Testing stopBackend fix ===');
startDummyBackend();

setTimeout(() => {
  const pid = backendProcess.pid;
  console.log('Before stopBackend, process running:', isRunning(pid));

  stopBackend();

  // Wait a moment for process to actually die
  setTimeout(() => {
    const stillRunning = isRunning(pid);
    console.log('After stopBackend, process running:', stillRunning);

    if (stillRunning) {
      console.log('FAIL: Backend process was NOT killed.');
      process.exit(1);
    } else {
      console.log('PASS: Backend process was killed successfully.');
      process.exit(0);
    }
  }, 500);
}, 1000);
