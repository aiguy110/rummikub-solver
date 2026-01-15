// Web Worker for running the Rummikub solver in a background thread
// This prevents blocking the main UI thread during computation

let wasmModule = null;

// Initialize WASM module in the worker
async function initWasm() {
    try {
        const wasm = await import('./pkg/rummikub_solver.js');
        await wasm.default();
        wasmModule = wasm;
        console.log('[Worker] WASM module loaded successfully');

        // Notify main thread that worker is ready
        self.postMessage({ type: 'ready' });
    } catch (error) {
        console.error('[Worker] Failed to load WASM:', error);
        self.postMessage({
            type: 'error',
            error: 'Failed to load solver module: ' + error.message
        });
    }
}

// Handle messages from main thread
self.onmessage = function(e) {
    const { type, data } = e.data;

    if (type === 'solve') {
        if (!wasmModule) {
            self.postMessage({
                type: 'error',
                error: 'WASM module not loaded yet'
            });
            return;
        }

        try {
            const { handArray, table, strategy, timeLimit } = data;

            // Call WASM solver
            const resultJson = wasmModule.solve_rummikub(
                JSON.stringify(handArray),
                JSON.stringify(table),
                strategy,
                BigInt(timeLimit)
            );

            const result = JSON.parse(resultJson);

            // Send result back to main thread
            self.postMessage({
                type: 'result',
                result: result
            });
        } catch (error) {
            console.error('[Worker] Solver error:', error);
            self.postMessage({
                type: 'error',
                error: 'Solver error: ' + error.message
            });
        }
    }
};

// Start initialization
initWasm();
