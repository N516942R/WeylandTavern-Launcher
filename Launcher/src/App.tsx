import { useCallback, useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';

const appWindow = getCurrentWindow();

type UpdateStatus = 'success' | 'upToDate' | 'needRetry' | 'failed';

interface UpdateResponse {
  status: UpdateStatus;
  message: string;
  logPath?: string;
  diff?: string;
  stashUsed: boolean;
}

interface CharacterResponse {
  success: boolean;
  message: string;
}

type Step =
  | 'updating'
  | 'updateRetry'
  | 'updateFailed'
  | 'stashPrompt'
  | 'characterRunning'
  | 'launching';

type ProgressStatus = 'pending' | 'running' | 'success' | 'error' | 'skipped';

function App() {
  const [ready, setReady] = useState(false);
  const [url, setUrl] = useState('');
  const [logs, setLogs] = useState<string[]>([]);
  const [showLogs, setShowLogs] = useState(false);
  const [step, setStep] = useState<Step>('updating');
  const [updateResult, setUpdateResult] = useState<UpdateResponse | null>(null);
  const [characterResult, setCharacterResult] = useState<CharacterResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [serverError, setServerError] = useState<string | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [serverRequested, setServerRequested] = useState(false);
  const [characterRequested, setCharacterRequested] = useState(false);
  const [hasTriggeredInitialUpdate, setHasTriggeredInitialUpdate] = useState(false);

  useEffect(() => {
    const unlistenReady = listen<string>('server-ready', (e) => {
      setUrl(e.payload);
      setReady(true);
    });
    const unlistenLog = listen<string>('log', (e) => {
      setLogs((prev) => [...prev, e.payload]);
    });

    const handler = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === 'r') {
        window.location.reload();
      } else if (e.ctrlKey && e.key.toLowerCase() === 'l') {
        e.preventDefault();
        setShowLogs((v) => !v);
      } else if (e.ctrlKey && e.key.toLowerCase() === 'q') {
        e.preventDefault();
        void appWindow.close();
      }
    };
    window.addEventListener('keydown', handler);

    return () => {
      unlistenReady.then((f) => f());
      unlistenLog.then((f) => f());
      window.removeEventListener('keydown', handler);
    };
  }, []);

  useEffect(() => {
    if (step === 'launching' && !serverRequested) {
      setServerRequested(true);
      setServerError(null);
      void invoke('start_server')
        .catch((err) => {
          setServerError(err instanceof Error ? err.message : String(err));
        });
    }
  }, [step, serverRequested]);

  const handleUpdate = async (attemptOverwrite: boolean) => {
    setError(null);
    setIsProcessing(true);
    setStep('updating');
    try {
      const result = await invoke<UpdateResponse>('update_vendor', { attemptOverwrite });
      setUpdateResult(result);
      if (result.status === 'success' || result.status === 'upToDate') {
        if (result.stashUsed) {
          setStep('stashPrompt');
        } else {
          setCharacterResult(null);
          setCharacterRequested(false);
          setStep('characterRunning');
        }
      } else if (result.status === 'needRetry') {
        setStep('updateRetry');
      } else {
        setStep('updateFailed');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setStep('updateFailed');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRetryWithOverwrite = () => {
    void handleUpdate(true);
  };

  const handleSkipAfterFailure = () => {
    setUpdateResult((prev) =>
      prev ? { ...prev, message: 'WeylandTavern failed to update.' } : prev
    );
    setStep('updateFailed');
  };

  const handleStartAnyway = () => {
    setCharacterResult(null);
    setCharacterRequested(false);
    setStep('characterRunning');
  };

  const handleExit = () => {
    void appWindow.close();
  };

  const handleFinalizeStash = async (revert: boolean) => {
    setError(null);
    setIsProcessing(true);
    try {
      await invoke('finalize_stash', { revert });
      setCharacterResult(null);
      setCharacterRequested(false);
      setStep('characterRunning');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const runCharacterSync = useCallback(async () => {
    setError(null);
    setCharacterResult(null);
    setIsProcessing(true);
    try {
      const result = await invoke<CharacterResponse>('run_character_sync');
      setCharacterResult(result);
    } catch (err) {
      setCharacterResult({
        success: false,
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setIsProcessing(false);
      setCharacterRequested(false);
      setStep('launching');
    }
  }, []);

  const retryServer = () => {
    setServerError(null);
    setServerRequested(false);
  };

  const buttonRowStyle = useMemo(
    () => ({ display: 'flex', gap: '0.75rem', marginTop: '1rem', flexWrap: 'wrap' as const }),
    []
  );

  useEffect(() => {
    if (step === 'characterRunning' && !characterRequested) {
      setCharacterRequested(true);
      void runCharacterSync();
    }
  }, [step, characterRequested, runCharacterSync]);

  useEffect(() => {
    if (!hasTriggeredInitialUpdate) {
      setHasTriggeredInitialUpdate(true);
      void handleUpdate(false);
    }
  }, [hasTriggeredInitialUpdate]);

  const statusLabels: Record<ProgressStatus, string> = useMemo(
    () => ({
      pending: 'Pending',
      running: 'In progress',
      success: 'Done',
      error: 'Attention required',
      skipped: 'Skipped',
    }),
    []
  );

  const statusColors: Record<ProgressStatus, string> = useMemo(
    () => ({
      pending: '#757575',
      running: '#42a5f5',
      success: '#66bb6a',
      error: '#ef5350',
      skipped: '#9e9e9e',
    }),
    []
  );

  const computeProgress = () => {
    let updateStatus: ProgressStatus = 'pending';
    let updateProgress = 0;
    let updateMessage: string | undefined;

    if (step === 'updating') {
      updateStatus = 'running';
      updateProgress = 50;
    }

    if (updateResult) {
      updateMessage = updateResult.message;
      if (updateResult.status === 'success' || updateResult.status === 'upToDate') {
        updateStatus = 'success';
        updateProgress = 100;
      } else if (updateResult.status === 'needRetry') {
        updateStatus = 'error';
        updateProgress = Math.max(updateProgress, 60);
      } else if (updateResult.status === 'failed') {
        updateStatus = 'error';
        updateProgress = 100;
      }
    } else if (step !== 'updating') {
      if (step === 'updateFailed' && error) {
        updateStatus = 'error';
        updateProgress = 100;
        updateMessage = error;
      } else if (step !== 'updateRetry') {
        updateStatus = 'success';
        updateProgress = 100;
      }
    }

    let characterStatus: ProgressStatus = 'pending';
    let characterProgress = 0;
    let characterMessage: string | undefined;

    if (step === 'characterRunning') {
      characterStatus = 'running';
      characterProgress = 50;
    }

    if (characterResult) {
      characterMessage = characterResult.message;
      characterStatus = characterResult.success ? 'success' : 'error';
      characterProgress = 100;
    }

    let serverStatus: ProgressStatus = 'pending';
    let serverProgress = 0;
    let serverMessage: string | undefined;

    if (ready) {
      serverStatus = 'success';
      serverProgress = 100;
      serverMessage = `Server ready at ${url}`;
    } else if (step === 'launching') {
      if (serverError) {
        serverStatus = 'error';
        serverProgress = 100;
        serverMessage = serverError;
      } else if (serverRequested) {
        serverStatus = 'running';
        serverProgress = 60;
        serverMessage = 'Starting WeylandTavern...';
      } else {
        serverStatus = 'pending';
        serverProgress = 0;
      }
    }

    return {
      update: { status: updateStatus, progress: updateProgress, message: updateMessage },
      character: { status: characterStatus, progress: characterProgress, message: characterMessage },
      server: { status: serverStatus, progress: serverProgress, message: serverMessage },
    };
  };

  const StepProgress = ({
    label,
    status,
    progress,
    message,
  }: {
    label: string;
    status: ProgressStatus;
    progress: number;
    message?: string;
  }) => (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '0.35rem',
        textAlign: 'left',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', fontWeight: 600 }}>
        <span>{label}</span>
        <span>{statusLabels[status]}</span>
      </div>
      <div
        style={{
          height: '0.5rem',
          width: '100%',
          borderRadius: '9999px',
          backgroundColor: '#2c2c2c',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            height: '100%',
            width: `${progress}%`,
            backgroundColor: statusColors[status],
            transition: 'width 0.3s ease',
          }}
        />
      </div>
      {message && (
        <span style={{ fontSize: '0.85rem', opacity: 0.8 }}>{message}</span>
      )}
    </div>
  );

  const renderProgress = () => {
    const { update, character, server } = computeProgress();
    return (
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: '0.75rem',
          width: '100%',
          maxWidth: '32rem',
        }}
      >
        <StepProgress label="Update" {...update} />
        <StepProgress label="Character sync" {...character} />
        <StepProgress label="Server" {...server} />
      </div>
    );
  };

  const renderUpdateDetails = () => {
    if (!updateResult || !updateResult.logPath) {
      return null;
    }
    return (
      <div style={{ marginTop: '0.75rem', maxWidth: '42rem' }}>
        <p>
          A log file was written to <code>{updateResult.logPath}</code>.
        </p>
        {updateResult.diff && (
          <pre
            style={{
              backgroundColor: '#111',
              color: '#0f0',
              padding: '0.75rem',
              borderRadius: '0.5rem',
              maxHeight: '12rem',
              overflow: 'auto',
            }}
          >
            {updateResult.diff}
          </pre>
        )}
      </div>
    );
  };

  const renderStepContent = () => {
    switch (step) {
      case 'updating':
        return <p>Updating WeylandTavern...</p>;
      case 'updateRetry':
        return (
          <>
            <p>There was an error updating WeylandTavern.</p>
            {renderUpdateDetails()}
            <div style={buttonRowStyle}>
              <button onClick={handleRetryWithOverwrite} disabled={isProcessing}>
                Retry with overwrite
              </button>
              <button onClick={handleSkipAfterFailure} disabled={isProcessing}>
                Skip update
              </button>
              <button onClick={handleExit} disabled={isProcessing}>
                Exit
              </button>
            </div>
          </>
        );
      case 'updateFailed':
        return (
          <>
            <p>{updateResult?.message ?? 'WeylandTavern failed to update.'}</p>
            <p>Start WeylandTavern anyway?</p>
            {renderUpdateDetails()}
            <div style={buttonRowStyle}>
              <button onClick={handleStartAnyway} disabled={isProcessing}>
                Start anyway
              </button>
              <button onClick={handleExit} disabled={isProcessing}>
                Exit
              </button>
            </div>
          </>
        );
      case 'stashPrompt':
        return (
          <>
            <p>Revert differing files post update?</p>
            <div style={buttonRowStyle}>
              <button onClick={() => void handleFinalizeStash(true)} disabled={isProcessing}>
                Yes
              </button>
              <button onClick={() => void handleFinalizeStash(false)} disabled={isProcessing}>
                No
              </button>
            </div>
          </>
        );
      case 'characterRunning':
        return <p>Checking for character updates...</p>;
      case 'launching':
        return (
          <>
            <p>Starting WeylandTavern...</p>
            {characterResult && (
              <p style={{ marginTop: '0.5rem' }}>{characterResult.message}</p>
            )}
            {serverError && (
              <div style={{ marginTop: '1rem' }}>
                <p style={{ color: '#ff8a80' }}>Failed to start the server: {serverError}</p>
                <button onClick={retryServer}>Retry start</button>
              </div>
            )}
          </>
        );
      default:
        return null;
    }
  };

  if (ready) {
    return (
      <div style={{ width: '100vw', height: '100vh' }}>
        <iframe src={url} style={{ border: 'none', width: '100%', height: '100%' }} />
        {showLogs && (
          <div
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              backgroundColor: 'rgba(0,0,0,0.8)',
              color: '#0f0',
              overflow: 'auto',
              padding: '1rem',
            }}
          >
            <pre>{logs.join('\n')}</pre>
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        minHeight: '100vh',
        padding: '2rem',
        gap: '1rem',
        textAlign: 'center',
      }}
    >
      <h1>WeylandTavern Launcher</h1>
      {renderProgress()}
      {renderStepContent()}
      {error && <p style={{ color: '#ff8a80' }}>{error}</p>}
      <p style={{ fontSize: '0.9rem', opacity: 0.75 }}>
        Use <kbd>Ctrl</kbd>+<kbd>L</kbd> to toggle logs, <kbd>Ctrl</kbd>+<kbd>R</kbd> to reload, and <kbd>Ctrl</kbd>+<kbd>Q</kbd> to quit.
      </p>
      {showLogs && (
        <div
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            backgroundColor: 'rgba(0,0,0,0.85)',
            color: '#0f0',
            overflow: 'auto',
            padding: '1rem',
          }}
        >
          <pre>{logs.join('\n')}</pre>
        </div>
      )}
    </div>
  );
}

export default App;
