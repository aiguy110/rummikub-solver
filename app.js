// Console log capturing - must be at the very top
const capturedLogs = [];
const MAX_LOGS = 500;

const originalConsoleLog = console.log;
const originalConsoleError = console.error;
const originalConsoleWarn = console.warn;

function captureLog(level, source, args) {
    const message = args.map(arg =>
        typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');

    capturedLogs.push({
        level,
        source,
        message,
        timestamp: new Date()
    });

    // Limit log size
    if (capturedLogs.length > MAX_LOGS) {
        capturedLogs.shift();
    }

    // Update display if logs modal is open
    updateLogsDisplay();
}

console.log = function(...args) {
    originalConsoleLog.apply(console, args);
    captureLog('log', 'main', args);
};

console.error = function(...args) {
    originalConsoleError.apply(console, args);
    captureLog('error', 'main', args);
};

console.warn = function(...args) {
    originalConsoleWarn.apply(console, args);
    captureLog('warn', 'main', args);
};

// State management
let hand = new Map(); // tile -> count
let table = []; // array of meld objects
let wasmModule = null;
let apiKey = null;
let modelName = 'google/gemini-3-flash-preview'; // default model
let currentImageMode = null; // 'hand' or 'table'
let handPrompt = null; // current hand prompt
let tablePrompt = null; // current table prompt
let processingQueue = []; // track ongoing photo processing
let nextProcessingId = 0; // unique ID for each processing request
let confirmationQueue = []; // pending confirmations
let nextConfirmationId = 0; // unique ID for confirmations

// Web Worker for solving
let solverWorker = null;
let solverWorkerReady = false;
let currentTimerWidget = null;
let currentTimerInterval = null;
let solverTimeoutId = null; // Timeout to detect worker crashes

// Initialize WASM
async function initWasm() {
    try {
        const wasm = await import('./pkg/rummikub_solver.js');
        await wasm.default();
        wasmModule = wasm;
        console.log('WASM module loaded successfully');

        // Update footer with WASM module build commit
        try {
            const buildCommit = wasm.get_build_commit();
            const wasmCommitElement = document.getElementById('wasm-commit');
            if (wasmCommitElement) {
                wasmCommitElement.textContent = buildCommit;
            }
        } catch (error) {
            console.error('Failed to get WASM build commit:', error);
            const wasmCommitElement = document.getElementById('wasm-commit');
            if (wasmCommitElement) {
                wasmCommitElement.textContent = 'unknown';
            }
        }
    } catch (error) {
        console.error('Failed to load WASM:', error);
        showError('Failed to load solver module. Make sure WASM files are built.');
    }
}

// Initialize Web Worker
function initWorker() {
    try {
        solverWorker = new Worker('./solver-worker.js', { type: 'module' });

        solverWorker.onmessage = function(e) {
            const { type, result, error, level, message } = e.data;

            if (type === 'ready') {
                solverWorkerReady = true;
                console.log('Solver worker ready');
            } else if (type === 'result') {
                handleSolverResult(result);
            } else if (type === 'error') {
                handleSolverError(error);
            } else if (type === 'log') {
                // Capture log from worker
                capturedLogs.push({
                    level: level || 'log',
                    source: 'worker',
                    message: message,
                    timestamp: new Date()
                });
                if (capturedLogs.length > MAX_LOGS) {
                    capturedLogs.shift();
                }
                updateLogsDisplay();
            }
        };

        solverWorker.onerror = function(error) {
            console.error('Worker error:', error);
            solverWorkerReady = false;
            showError('Solver worker error: ' + error.message);
        };
    } catch (error) {
        console.error('Failed to create worker:', error);
        showError('Failed to create solver worker. Falling back to main thread.');
        solverWorker = null;
    }
}

// Handle solver result from worker
function handleSolverResult(result) {
    const timeLimit = parseInt(document.getElementById('time-limit').value);

    // Clear worker timeout
    if (solverTimeoutId) {
        clearTimeout(solverTimeoutId);
        solverTimeoutId = null;
    }

    // Stop timer animation and remove widget
    if (currentTimerInterval) {
        clearInterval(currentTimerInterval);
        currentTimerInterval = null;
    }
    removeTimerWidget();

    // Re-enable solve button
    const solveBtn = document.getElementById('solve-btn');
    solveBtn.disabled = false;
    solveBtn.textContent = 'Find Best Moves';
    solveBtn.classList.remove('loading');

    // Show results
    showSolverResultToast(result, timeLimit);
    displayResults(result);
}

// Handle solver error from worker
function handleSolverError(error) {
    // Clear worker timeout
    if (solverTimeoutId) {
        clearTimeout(solverTimeoutId);
        solverTimeoutId = null;
    }

    // Stop timer animation and remove widget
    if (currentTimerInterval) {
        clearInterval(currentTimerInterval);
        currentTimerInterval = null;
    }
    removeTimerWidget();

    // Re-enable solve button
    const solveBtn = document.getElementById('solve-btn');
    solveBtn.disabled = false;
    solveBtn.textContent = 'Find Best Moves';
    solveBtn.classList.remove('loading');

    console.error('Solver error:', error);
    showError(error);
}

// Handle worker timeout/crash
function handleWorkerTimeout() {
    console.error('Worker timed out or crashed - no response received');

    solverTimeoutId = null;

    // Stop timer animation and remove widget
    if (currentTimerInterval) {
        clearInterval(currentTimerInterval);
        currentTimerInterval = null;
    }
    removeTimerWidget();

    // Re-enable solve button
    const solveBtn = document.getElementById('solve-btn');
    solveBtn.disabled = false;
    solveBtn.textContent = 'Find Best Moves';
    solveBtn.classList.remove('loading');

    showError('Solver worker crashed or became unresponsive. The page will be reloaded to recover.');

    // Reload after a short delay to allow user to see the error
    setTimeout(() => {
        window.location.reload();
    }, 3000);
}

// Initialize UI
function initUI() {
    createTilePicker();
    updateHandDisplay();
    updateTableDisplay();
    loadSavedStates();
    loadApiKey();
    attachEventListeners();
}

// Toast notification system
function showToast(title, message, type = 'info', duration = 5000) {
    const container = document.getElementById('toast-container');
    const toast = document.createElement('div');
    toast.className = `toast ${type}`;

    const icons = {
        success: '✅',
        error: '❌',
        info: 'ℹ️'
    };

    toast.innerHTML = `
        <div class="toast-icon">${icons[type] || icons.info}</div>
        <div class="toast-content">
            <div class="toast-title">${title}</div>
            ${message ? `<div class="toast-message">${message}</div>` : ''}
        </div>
        <button class="toast-close">×</button>
    `;

    const closeBtn = toast.querySelector('.toast-close');
    closeBtn.addEventListener('click', () => {
        toast.remove();
    });

    container.appendChild(toast);

    if (duration > 0) {
        setTimeout(() => {
            toast.remove();
        }, duration);
    }
}

// Check if two melds are identical
function meldsAreIdentical(meld1, meld2) {
    if (meld1.type !== meld2.type) return false;
    if (meld1.tiles.length !== meld2.tiles.length) return false;

    // Compare tiles (order matters for runs, but we'll check both orders)
    const tiles1 = meld1.tiles.join(',');
    const tiles2 = meld2.tiles.join(',');

    if (tiles1 === tiles2) return true;

    // Check reverse order (in case meld was read backwards)
    const tiles2Rev = meld2.tiles.slice().reverse().join(',');
    return tiles1 === tiles2Rev;
}

// Find duplicate melds on the table
function findDuplicateMeld(meld) {
    for (let i = 0; i < table.length; i++) {
        if (meldsAreIdentical(table[i], meld)) {
            return i;
        }
    }
    return -1;
}

// Add confirmation to queue
function addConfirmation(meld, reason) {
    const id = nextConfirmationId++;
    confirmationQueue.push({ id, meld, reason });
    updateConfirmationDisplay();
}

// Approve confirmation
function approveConfirmation(id) {
    const index = confirmationQueue.findIndex(c => c.id === id);
    if (index === -1) return;

    const confirmation = confirmationQueue[index];
    table.push(confirmation.meld);
    confirmationQueue.splice(index, 1);

    updateTableDisplay();
    updateConfirmationDisplay();
    showToast('Meld Added', 'Meld has been added to the table', 'success', 3000);
}

// Reject confirmation
function rejectConfirmation(id) {
    const index = confirmationQueue.findIndex(c => c.id === id);
    if (index === -1) return;

    confirmationQueue.splice(index, 1);
    updateConfirmationDisplay();
    showToast('Meld Rejected', 'Duplicate meld was not added', 'info', 3000);
}

// Update confirmation queue display
function updateConfirmationDisplay() {
    const section = document.getElementById('confirmation-section');
    const queue = document.getElementById('confirmation-queue');

    if (confirmationQueue.length === 0) {
        section.style.display = 'none';
        return;
    }

    section.style.display = 'block';
    queue.innerHTML = '';

    confirmationQueue.forEach(confirmation => {
        const item = document.createElement('div');
        item.className = 'confirmation-item';

        const meldTilesHtml = confirmation.meld.tiles
            .map(tile => `<span class="meld-tile ${getTileColor(tile)}">${formatTileDisplay(tile)}</span>`)
            .join('');

        item.innerHTML = `
            <div class="confirmation-header">
                <div class="confirmation-title">Potential Duplicate Meld</div>
            </div>
            <div class="confirmation-reason">${confirmation.reason}</div>
            <div class="confirmation-meld">
                <span class="meld-type-badge">${confirmation.meld.type}</span>
                <div class="meld-tiles">${meldTilesHtml}</div>
            </div>
            <div class="confirmation-actions">
                <button class="btn btn-approve" onclick="approveConfirmation(${confirmation.id})">Add Anyway</button>
                <button class="btn btn-reject" onclick="rejectConfirmation(${confirmation.id})">Skip</button>
            </div>
        `;

        queue.appendChild(item);
    });
}

// Create tile picker buttons
function createTilePicker() {
    const colors = [
        { name: 'red', code: 'r', element: document.getElementById('red-tiles') },
        { name: 'blue', code: 'b', element: document.getElementById('blue-tiles') },
        { name: 'yellow', code: 'y', element: document.getElementById('yellow-tiles') },
        { name: 'black', code: 'k', element: document.getElementById('black-tiles') }
    ];

    colors.forEach(({ name, code, element }) => {
        for (let num = 1; num <= 13; num++) {
            const btn = document.createElement('button');
            btn.className = `tile-btn ${name}`;
            btn.dataset.tile = `${code}${num}`;
            btn.textContent = num;
            btn.addEventListener('click', () => addTileToHand(btn.dataset.tile));
            element.appendChild(btn);
        }
    });

    // Create wild button
    const wildElement = document.getElementById('wild-tiles');
    const wildBtn = document.createElement('button');
    wildBtn.className = 'tile-btn wild-btn';
    wildBtn.dataset.tile = 'w';
    wildBtn.textContent = 'W';
    wildBtn.addEventListener('click', () => addTileToHand('w'));
    wildElement.appendChild(wildBtn);
}

// Add tile to hand
function addTileToHand(tileStr) {
    const currentCount = hand.get(tileStr) || 0;
    hand.set(tileStr, currentCount + 1);
    updateHandDisplay();
    updateTileCounts();
}

// Remove tile from hand
function removeTileFromHand(tileStr) {
    const currentCount = hand.get(tileStr) || 0;
    if (currentCount > 1) {
        hand.set(tileStr, currentCount - 1);
    } else {
        hand.delete(tileStr);
    }
    updateHandDisplay();
    updateTileCounts();
}

// Update tile button counts
function updateTileCounts() {
    document.querySelectorAll('.tile-btn').forEach(btn => {
        const tile = btn.dataset.tile;
        const count = hand.get(tile) || 0;

        // Remove existing count badge
        const existingBadge = btn.querySelector('.count');
        if (existingBadge) {
            existingBadge.remove();
        }

        // Add count badge if > 0
        if (count > 0) {
            const badge = document.createElement('span');
            badge.className = 'count';
            badge.textContent = count;
            btn.appendChild(badge);
        }
    });
}

// Sort tiles by color (red, blue, yellow, black, wild) then by number
function sortTiles(tileA, tileB) {
    // Define color order matching tile picker: red, blue, yellow, black, wild
    const colorOrder = { 'r': 0, 'b': 1, 'y': 2, 'k': 3, 'w': 4 };

    const colorA = tileA[0];
    const colorB = tileB[0];

    // First compare by color
    const colorCompare = colorOrder[colorA] - colorOrder[colorB];
    if (colorCompare !== 0) {
        return colorCompare;
    }

    // If same color, compare by number
    // Wild tiles don't have numbers, so they're already sorted by color
    if (colorA === 'w') return 0;

    const numA = parseInt(tileA.substring(1));
    const numB = parseInt(tileB.substring(1));
    return numA - numB;
}

// Update hand display
function updateHandDisplay() {
    const display = document.getElementById('hand-display');
    const countSpan = document.getElementById('hand-count');

    // Calculate total tiles
    let totalTiles = 0;
    hand.forEach(count => totalTiles += count);
    countSpan.textContent = totalTiles;

    if (hand.size === 0) {
        display.innerHTML = '<p class="empty-message">Tap tiles above to add them to your hand</p>';
        return;
    }

    const tilesDiv = document.createElement('div');
    tilesDiv.className = 'hand-tiles';

    // Sort tiles by color and number
    const sortedTiles = Array.from(hand.entries()).sort((a, b) => {
        return sortTiles(a[0], b[0]);
    });

    sortedTiles.forEach(([tile, count]) => {
        for (let i = 0; i < count; i++) {
            const tileDiv = document.createElement('div');
            tileDiv.className = `hand-tile ${getTileColor(tile)}`;
            tileDiv.innerHTML = `
                ${formatTileDisplay(tile)}
                <button class="remove-btn" onclick="removeTileFromHand('${tile}')">×</button>
            `;
            tilesDiv.appendChild(tileDiv);
        }
    });

    display.innerHTML = '';
    display.appendChild(tilesDiv);
}

// Get tile color from tile string
function getTileColor(tile) {
    if (tile === 'w') return 'wild';
    const colorMap = { 'r': 'red', 'b': 'blue', 'y': 'yellow', 'k': 'black' };
    return colorMap[tile[0]] || 'black';
}

// Format tile for display
function formatTileDisplay(tile) {
    if (tile === 'w') return 'W';
    const colorMap = { 'r': 'R', 'b': 'B', 'y': 'Y', 'k': 'K' };
    return `${colorMap[tile[0]]}${tile.substring(1)}`;
}

// Parse group meld: "5 r b k" or "b5 r8 k5" format
function parseGroupTiles(input) {
    const parts = input.toLowerCase().split(/\s+/).filter(p => p);

    if (parts.length < 3) {
        throw new Error('Group must have at least 3 tiles');
    }

    // Check if first part is color+number format (e.g., "b5")
    const firstPart = parts[0];
    const isColorNumberFormat = /^[rbykw]\d*$/.test(firstPart) || firstPart === 'w';

    if (isColorNumberFormat) {
        // Format: "b5 r8 k5" or "b5 r8 w"
        const tiles = [];
        for (const part of parts) {
            if (part === 'w') {
                tiles.push('w');
            } else {
                const match = part.match(/^([rbyk])(\d{1,2})$/);
                if (!match) {
                    throw new Error(`Invalid tile format: ${part}. Use format like b5, r12, or w.`);
                }
                const color = match[1];
                const num = parseInt(match[2]);
                if (num < 1 || num > 13) {
                    throw new Error(`Invalid number in ${part}. Must be 1-13.`);
                }
                tiles.push(`${color}${num}`);
            }
        }
        return tiles;
    } else {
        // Format: "5 r b k" or "5 r b w"
        if (parts.length < 4) {
            throw new Error('Group must have at least 4 parts (number + 3 colors)');
        }

        const number = parts[0];
        const colors = parts.slice(1);

        // Validate number
        const numVal = parseInt(number);
        if (isNaN(numVal) || numVal < 1 || numVal > 13) {
            throw new Error(`Invalid number: ${number}. Must be 1-13.`);
        }

        // Validate colors and create tiles
        const tiles = [];
        for (const color of colors) {
            if (!['r', 'b', 'y', 'k', 'w'].includes(color)) {
                throw new Error(`Invalid color: ${color}. Use r, b, y, k, or w.`);
            }
            if (color === 'w') {
                tiles.push('w');
            } else {
                tiles.push(`${color}${numVal}`);
            }
        }

        return tiles;
    }
}

// Parse run meld: "y 6 7 8" or "y6 y7 y8" format
function parseRunTiles(input) {
    const parts = input.toLowerCase().split(/\s+/).filter(p => p);

    if (parts.length < 3) {
        throw new Error('Run must have at least 3 tiles');
    }

    // Check if first part is color+number format (e.g., "y6")
    const firstPart = parts[0];
    const isColorNumberFormat = /^[rbykw]\d+$/.test(firstPart) || firstPart === 'w';

    if (isColorNumberFormat) {
        // Format: "y6 y7 y8" or "y6 y7 w"
        const tiles = [];
        for (const part of parts) {
            if (part === 'w') {
                tiles.push('w');
            } else {
                const match = part.match(/^([rbyk])(\d{1,2})$/);
                if (!match) {
                    throw new Error(`Invalid tile format: ${part}. Use format like y6, b12, or w.`);
                }
                const color = match[1];
                const num = parseInt(match[2]);
                if (num < 1 || num > 13) {
                    throw new Error(`Invalid number in ${part}. Must be 1-13.`);
                }
                tiles.push(`${color}${num}`);
            }
        }
        return tiles;
    } else {
        // Format: "y 6 7 8" or "y 6 7 w"
        if (parts.length < 4) {
            throw new Error('Run must have at least 4 parts (color + 3 numbers)');
        }

        const color = parts[0];
        const numbers = parts.slice(1);

        // Validate color
        if (!['r', 'b', 'y', 'k', 'w'].includes(color)) {
            throw new Error(`Invalid color: ${color}. Use r, b, y, k, or w.`);
        }

        // Validate numbers and create tiles
        const tiles = [];
        for (const numStr of numbers) {
            if (numStr === 'w') {
                tiles.push('w');
            } else {
                const num = parseInt(numStr);
                if (isNaN(num) || num < 1 || num > 13) {
                    throw new Error(`Invalid number: ${numStr}. Must be 1-13.`);
                }
                if (color === 'w') {
                    tiles.push('w');
                } else {
                    tiles.push(`${color}${num}`);
                }
            }
        }

        return tiles;
    }
}

// Add meld to table
function addMeldToTable() {
    const typeSelect = document.getElementById('meld-type');
    const tilesInput = document.getElementById('meld-tiles');

    const type = typeSelect.value;
    const tilesStr = tilesInput.value.trim();

    if (!tilesStr) {
        showError('Please enter tiles for the meld');
        return;
    }

    try {
        let tiles;
        if (type === 'group') {
            tiles = parseGroupTiles(tilesStr);
        } else if (type === 'run') {
            tiles = parseRunTiles(tilesStr);
        } else {
            showError('Invalid meld type');
            return;
        }

        if (tiles.length < 3) {
            showError('A meld must have at least 3 tiles');
            return;
        }

        const meld = { type, tiles };
        table.push(meld);

        updateTableDisplay();
        tilesInput.value = '';
    } catch (error) {
        showError(`Invalid input: ${error.message}`);
    }
}

// Validate tile string
function isValidTile(tile) {
    if (tile === 'w') return true;
    const match = tile.match(/^([rbyk])(\d{1,2})$/);
    if (!match) return false;
    const num = parseInt(match[2]);
    return num >= 1 && num <= 13;
}

// Remove meld from table
function removeMeldFromTable(index) {
    table.splice(index, 1);
    updateTableDisplay();
}

// Update table display
function updateTableDisplay() {
    const display = document.getElementById('table-display');

    if (table.length === 0) {
        display.innerHTML = '<p class="empty-message">No melds on the table</p>';
        return;
    }

    display.innerHTML = '';

    table.forEach((meld, index) => {
        const meldDiv = document.createElement('div');
        meldDiv.className = 'table-meld';

        const badge = document.createElement('span');
        badge.className = 'meld-type-badge';
        badge.textContent = meld.type;

        const tilesDiv = document.createElement('div');
        tilesDiv.className = 'meld-tiles';

        meld.tiles.forEach(tile => {
            const tileSpan = document.createElement('span');
            tileSpan.className = `meld-tile ${getTileColor(tile)}`;
            tileSpan.textContent = formatTileDisplay(tile);
            tilesDiv.appendChild(tileSpan);
        });

        const removeBtn = document.createElement('button');
        removeBtn.className = 'remove-meld-btn';
        removeBtn.textContent = 'Remove';
        removeBtn.onclick = () => removeMeldFromTable(index);

        meldDiv.appendChild(badge);
        meldDiv.appendChild(tilesDiv);
        meldDiv.appendChild(removeBtn);

        display.appendChild(meldDiv);
    });
}

// Circular Timer Widget Functions
function createTimerWidget(timeLimitMs) {
    const solveBtn = document.getElementById('solve-btn');
    const btnContainer = solveBtn.parentElement;

    // Create timer container
    const timerContainer = document.createElement('div');
    timerContainer.id = 'timer-widget';
    timerContainer.className = 'timer-widget';

    // Create SVG
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', '32');
    svg.setAttribute('height', '32');
    svg.setAttribute('viewBox', '0 0 32 32');

    // Background circle
    const bgCircle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    bgCircle.setAttribute('cx', '16');
    bgCircle.setAttribute('cy', '16');
    bgCircle.setAttribute('r', '14');
    bgCircle.setAttribute('fill', 'none');
    bgCircle.setAttribute('stroke', '#e0e0e0');
    bgCircle.setAttribute('stroke-width', '3');

    // Progress circle
    const progressCircle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    progressCircle.setAttribute('cx', '16');
    progressCircle.setAttribute('cy', '16');
    progressCircle.setAttribute('r', '14');
    progressCircle.setAttribute('fill', 'none');
    progressCircle.setAttribute('stroke', '#27ae60');
    progressCircle.setAttribute('stroke-width', '3');
    progressCircle.setAttribute('stroke-linecap', 'round');
    progressCircle.setAttribute('transform', 'rotate(-90 16 16)');

    // Calculate circumference
    const circumference = 2 * Math.PI * 14;
    progressCircle.setAttribute('stroke-dasharray', circumference);
    progressCircle.setAttribute('stroke-dashoffset', '0');
    progressCircle.id = 'timer-progress-circle';

    svg.appendChild(bgCircle);
    svg.appendChild(progressCircle);
    timerContainer.appendChild(svg);

    // Insert timer after the button
    btnContainer.appendChild(timerContainer);

    return {
        container: timerContainer,
        progressCircle,
        circumference,
        startTime: Date.now(),
        timeLimitMs
    };
}

function updateTimerWidget(timerWidget) {
    const elapsed = Date.now() - timerWidget.startTime;
    const progress = Math.min(elapsed / timerWidget.timeLimitMs, 1);
    const offset = timerWidget.circumference * progress;
    timerWidget.progressCircle.setAttribute('stroke-dashoffset', offset);
}

function removeTimerWidget() {
    const timerWidget = document.getElementById('timer-widget');
    if (timerWidget) {
        timerWidget.remove();
    }
}

// Solve the game
async function solve() {
    // Get total tiles in hand
    let totalTiles = 0;
    hand.forEach(count => totalTiles += count);

    if (totalTiles === 0) {
        showError('Please add tiles to your hand');
        return;
    }

    // Check if worker is available and ready
    if (!solverWorker || !solverWorkerReady) {
        showError('Solver not ready yet. Please wait a moment and try again.');
        return;
    }

    const strategy = document.getElementById('strategy').value;
    const timeLimit = parseInt(document.getElementById('time-limit').value);

    const solveBtn = document.getElementById('solve-btn');
    solveBtn.disabled = true;
    solveBtn.textContent = 'Solving';
    solveBtn.classList.add('loading');

    // Create and start timer widget
    currentTimerWidget = createTimerWidget(timeLimit);
    currentTimerInterval = setInterval(() => {
        updateTimerWidget(currentTimerWidget);
    }, 50); // Update every 50ms for smooth animation

    // Convert hand to array format
    const handArray = [];
    hand.forEach((count, tile) => {
        for (let i = 0; i < count; i++) {
            handArray.push(tile);
        }
    });

    // Set up a timeout to detect if worker crashes or hangs
    // Give it extra time beyond the configured limit (2x + 5 seconds buffer)
    const workerTimeoutMs = (timeLimit * 2) + 5000;
    solverTimeoutId = setTimeout(handleWorkerTimeout, workerTimeoutMs);

    // Send solve request to worker
    solverWorker.postMessage({
        type: 'solve',
        data: {
            handArray,
            table,
            strategy,
            timeLimit
        }
    });
}

function showSolverResultToast(result, timeLimit) {
    const completionReason = result.search_completed ? 'Search Complete' : 'Timeout';

    let title, message, type;

    if (result.success) {
        title = 'Solution Found!';
        const improvement = result.final_quality - result.initial_quality;
        const improvementText = improvement > 0
            ? `Improved by ${improvement} ${result.initial_quality < 0 ? 'tiles' : 'points'}`
            : 'No improvement';

        message = `
            <strong>${completionReason}</strong><br>
            Depth: ${result.depth_reached}<br>
            ${improvementText}<br>
            Time: ${timeLimit}ms
        `;
        type = 'success';
    } else {
        title = 'No Solution Found';
        message = `
            <strong>${completionReason}</strong><br>
            Depth reached: ${result.depth_reached}<br>
            Time: ${timeLimit}ms
        `;
        type = 'info';
    }

    showToast(title, message, type, 5000);
}

// Render tiles as HTML with consistent styling
function renderTilesAsHtml(tiles) {
    return tiles.map(tile => {
        const color = getTileColor(tile);
        const display = formatTileDisplay(tile);
        return `<span class="meld-tile ${color}">${display}</span>`;
    }).join('');
}

// Render a meld as HTML
function renderMeldAsHtml(meld) {
    const tilesHtml = renderTilesAsHtml(meld.tiles);
    return `<span class="meld-type-badge">${meld.type}</span> <span class="meld-tiles">${tilesHtml}</span>`;
}

// Render a human-readable move as HTML
function renderHumanMoveAsHtml(move) {
    switch (move.type) {
        case 'play_from_hand':
            return `Play from hand: ${renderMeldAsHtml(move.meld)}`;

        case 'extend_meld': {
            const addedHtml = renderTilesAsHtml(move.added_tiles);
            return `Extend ${renderMeldAsHtml(move.original)} by adding <span class="meld-tiles">${addedHtml}</span> → ${renderMeldAsHtml(move.result)}`;
        }

        case 'take_from_meld': {
            const takenHtml = renderTilesAsHtml(move.taken_tiles);
            return `Take <span class="meld-tiles">${takenHtml}</span> from ${renderMeldAsHtml(move.original)}, leaving ${renderMeldAsHtml(move.remaining)}`;
        }

        case 'split_meld': {
            const partsHtml = move.parts.map(renderMeldAsHtml).join(' and ');
            return `Split ${renderMeldAsHtml(move.original)} into ${partsHtml}`;
        }

        case 'join_melds': {
            const sourcesHtml = move.sources.map(renderMeldAsHtml).join(' + ');
            return `Combine ${sourcesHtml} → ${renderMeldAsHtml(move.result)}`;
        }

        case 'swap_wild': {
            const swapsHtml = move.swaps.map(s =>
                `<span class="meld-tiles">${renderTilesAsHtml([s.replacement])}</span> for <span class="meld-tiles">${renderTilesAsHtml([s.wild_taken])}</span>`
            ).join(', ');
            return `Swap ${swapsHtml} in ${renderMeldAsHtml(move.original)} → ${renderMeldAsHtml(move.result)}`;
        }

        case 'rearrange': {
            const consumedHtml = move.consumed.length > 0
                ? move.consumed.map(renderMeldAsHtml).join(', ')
                : 'nothing';
            const producedHtml = move.produced.map(renderMeldAsHtml).join(', ');
            const handHtml = move.hand_tiles_used.length > 0
                ? ` using <span class="meld-tiles">${renderTilesAsHtml(move.hand_tiles_used)}</span> from hand`
                : '';
            return `Rearrange ${consumedHtml}${handHtml} → ${producedHtml}`;
        }

        default:
            return `Unknown move type: ${move.type}`;
    }
}

// Display solver results
function displayResults(result) {
    const section = document.getElementById('results-section');
    const display = document.getElementById('results-display');

    section.style.display = 'block';

    if (!result.success) {
        display.innerHTML = `
            <div class="result-error">
                ${result.error || 'No solution found'}
            </div>
        `;
        return;
    }

    const humanMoves = result.human_moves || [];
    const rawMoves = result.moves || [];

    if (humanMoves.length === 0 && rawMoves.length === 0) {
        display.innerHTML = `
            <div class="result-success">
                No moves needed - your hand is already optimal!
            </div>
        `;
        return;
    }

    let html = `
        <div class="result-success">
            Found solution with ${humanMoves.length} action${humanMoves.length !== 1 ? 's' : ''}!
        </div>
    `;

    // Display human-readable moves
    if (humanMoves.length > 0) {
        html += `<ol class="move-list human-moves">`;
        humanMoves.forEach((move, index) => {
            html += `<li class="move-item">`;
            html += `<span class="move-number">${index + 1}.</span>`;
            html += renderHumanMoveAsHtml(move);
            html += `</li>`;
        });
        html += '</ol>';
    }

    // Add collapsible raw moves section for debugging
    if (rawMoves.length > 0) {
        html += `
            <details class="raw-moves-section">
                <summary>Show raw solver moves (${rawMoves.length})</summary>
                <ol class="move-list raw-moves">
        `;

        rawMoves.forEach((move, index) => {
            html += `<li class="move-item">`;
            html += `<span class="move-number">${index + 1}.</span>`;

            if (move.action === 'pickup') {
                const meldIndex = move.index;
                if (meldIndex >= 0 && meldIndex < table.length) {
                    const meld = table[meldIndex];
                    const tilesHtml = renderTilesAsHtml(meld.tiles);
                    html += `Pick up <span class="meld-type-badge">${meld.type}</span> <span class="meld-tiles">${tilesHtml}</span>`;
                } else {
                    html += `Pick up meld #${meldIndex + 1} from the table`;
                }
            } else if (move.action === 'laydown') {
                const meld = move.meld;
                const tilesHtml = renderTilesAsHtml(meld.tiles);
                html += `Lay down <span class="meld-type-badge">${meld.type}</span> <span class="meld-tiles">${tilesHtml}</span>`;
            }

            html += `</li>`;
        });

        html += '</ol></details>';
    }

    display.innerHTML = html;

    // Scroll to results
    section.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
}

// Show error message
function showError(message) {
    const section = document.getElementById('results-section');
    const display = document.getElementById('results-display');

    section.style.display = 'block';
    display.innerHTML = `<div class="result-error">${message}</div>`;
}

// Save current state
function saveState() {
    const nameInput = document.getElementById('save-name');
    const name = nameInput.value.trim() || `Game ${new Date().toLocaleString()}`;

    const state = {
        name,
        timestamp: Date.now(),
        hand: Array.from(hand.entries()),
        table: table,
        strategy: document.getElementById('strategy').value,
        timeLimit: document.getElementById('time-limit').value
    };

    const savedStates = getSavedStates();
    savedStates.push(state);
    localStorage.setItem('rummikub-saved-states', JSON.stringify(savedStates));

    nameInput.value = '';
    loadSavedStates();

    showToast('State Saved', 'Game state saved successfully', 'success', 3000);
}

// Get saved states from localStorage
function getSavedStates() {
    const saved = localStorage.getItem('rummikub-saved-states');
    return saved ? JSON.parse(saved) : [];
}

// Load saved states list
function loadSavedStates() {
    const container = document.getElementById('saved-states');
    const states = getSavedStates();

    if (states.length === 0) {
        container.innerHTML = '<p class="empty-message">No saved states</p>';
        return;
    }

    container.innerHTML = '';

    states.forEach((state, index) => {
        const item = document.createElement('div');
        item.className = 'saved-state-item';

        const date = new Date(state.timestamp);

        item.innerHTML = `
            <div>
                <div class="saved-state-name">${state.name}</div>
                <div class="saved-state-time">${date.toLocaleString()}</div>
            </div>
            <button class="delete-saved-btn" onclick="deleteSavedState(${index})">Delete</button>
        `;

        item.addEventListener('click', (e) => {
            if (!e.target.classList.contains('delete-saved-btn')) {
                loadState(index);
            }
        });

        container.appendChild(item);
    });
}

// Load a saved state
function loadState(index) {
    const states = getSavedStates();
    const state = states[index];

    if (!state) return;

    // Restore hand
    hand.clear();
    state.hand.forEach(([tile, count]) => {
        hand.set(tile, count);
    });

    // Restore table
    table = state.table;

    // Restore settings
    document.getElementById('strategy').value = state.strategy;
    document.getElementById('time-limit').value = state.timeLimit;

    updateHandDisplay();
    updateTileCounts();
    updateTableDisplay();

    showToast('State Loaded', 'Game state loaded successfully', 'success', 3000);
}

// Delete a saved state
function deleteSavedState(index) {
    const states = getSavedStates();
    states.splice(index, 1);
    localStorage.setItem('rummikub-saved-states', JSON.stringify(states));
    loadSavedStates();
}

// Show success message (legacy - redirects to toast)
function showSuccess(message) {
    showToast('Success', message, 'success', 3000);
}

// API Key Management
function loadApiKey() {
    apiKey = localStorage.getItem('rummikub-api-key');
    modelName = localStorage.getItem('rummikub-model') || 'openai/gpt-4o';
    loadPrompts();
    loadTimeLimit();
    updateCaptureButtonVisibility();
}

function loadTimeLimit() {
    const savedTimeLimit = localStorage.getItem('rummikub-time-limit');
    if (savedTimeLimit) {
        document.getElementById('time-limit').value = savedTimeLimit;
    }
}

function saveTimeLimit() {
    const timeLimit = document.getElementById('time-limit').value;
    localStorage.setItem('rummikub-time-limit', timeLimit);
}

function loadPrompts() {
    handPrompt = localStorage.getItem('rummikub-hand-prompt') || DEFAULT_HAND_PROMPT;
    tablePrompt = localStorage.getItem('rummikub-table-prompt') || DEFAULT_TABLE_PROMPT;
}

function saveApiKey() {
    const keyInput = document.getElementById('api-key-input');
    const modelInput = document.getElementById('model-input');
    const handPromptInput = document.getElementById('hand-prompt-input');
    const tablePromptInput = document.getElementById('table-prompt-input');

    const key = keyInput.value.trim();
    const model = modelInput.value.trim();
    const handPromptValue = handPromptInput.value.trim();
    const tablePromptValue = tablePromptInput.value.trim();

    if (!key) {
        showError('Please enter an API key');
        return;
    }

    if (!model) {
        showError('Please enter a model name');
        return;
    }

    localStorage.setItem('rummikub-api-key', key);
    localStorage.setItem('rummikub-model', model);

    // Save prompts (use defaults if empty)
    localStorage.setItem('rummikub-hand-prompt', handPromptValue || DEFAULT_HAND_PROMPT);
    localStorage.setItem('rummikub-table-prompt', tablePromptValue || DEFAULT_TABLE_PROMPT);

    apiKey = key;
    modelName = model;
    handPrompt = handPromptValue || DEFAULT_HAND_PROMPT;
    tablePrompt = tablePromptValue || DEFAULT_TABLE_PROMPT;

    keyInput.value = '';

    document.getElementById('settings-modal').style.display = 'none';
    updateCaptureButtonVisibility();
    showToast('Settings Saved', 'Your settings have been saved successfully', 'success', 3000);
}

function restoreDefaultPrompts() {
    const handPromptInput = document.getElementById('hand-prompt-input');
    const tablePromptInput = document.getElementById('table-prompt-input');

    handPromptInput.value = DEFAULT_HAND_PROMPT;
    tablePromptInput.value = DEFAULT_TABLE_PROMPT;

    showToast('Defaults Restored', 'Click "Save Settings" to apply the default prompts', 'info', 4000);
}

function updateCaptureButtonVisibility() {
    const cameraHandBtn = document.getElementById('camera-hand-btn');
    const galleryHandBtn = document.getElementById('gallery-hand-btn');
    const cameraTableBtn = document.getElementById('camera-table-btn');
    const galleryTableBtn = document.getElementById('gallery-table-btn');

    // Check if all buttons exist
    if (!cameraHandBtn || !galleryHandBtn || !cameraTableBtn || !galleryTableBtn) {
        console.error('One or more capture buttons not found in DOM');
        return;
    }

    if (apiKey) {
        cameraHandBtn.style.display = 'inline-block';
        galleryHandBtn.style.display = 'inline-block';
        cameraTableBtn.style.display = 'inline-block';
        galleryTableBtn.style.display = 'inline-block';
    } else {
        cameraHandBtn.style.display = 'none';
        galleryHandBtn.style.display = 'none';
        cameraTableBtn.style.display = 'none';
        galleryTableBtn.style.display = 'none';
    }
}

// Settings Modal Management
function openSettingsModal() {
    const modal = document.getElementById('settings-modal');
    const keyInput = document.getElementById('api-key-input');
    const modelInput = document.getElementById('model-input');
    const handPromptInput = document.getElementById('hand-prompt-input');
    const tablePromptInput = document.getElementById('table-prompt-input');

    if (apiKey) {
        keyInput.value = apiKey;
    }

    modelInput.value = modelName;
    handPromptInput.value = handPrompt || DEFAULT_HAND_PROMPT;
    tablePromptInput.value = tablePrompt || DEFAULT_TABLE_PROMPT;

    modal.style.display = 'flex';
}

function closeSettingsModal() {
    document.getElementById('settings-modal').style.display = 'none';
}

// Menu Management
let menuOpen = false;

function toggleMenu() {
    const dropdown = document.getElementById('menu-dropdown');
    menuOpen = !menuOpen;
    dropdown.style.display = menuOpen ? 'block' : 'none';
}

function closeMenu() {
    const dropdown = document.getElementById('menu-dropdown');
    menuOpen = false;
    dropdown.style.display = 'none';
}

// Logs Modal Management
function openLogsModal() {
    closeMenu();
    document.getElementById('logs-modal').style.display = 'flex';
    updateLogsDisplay();
}

function closeLogsModal() {
    document.getElementById('logs-modal').style.display = 'none';
}

function clearLogs() {
    capturedLogs.length = 0;
    updateLogsDisplay();
}

function updateLogsDisplay() {
    const container = document.getElementById('logs-container');
    if (!container) return;

    // Only update if modal is visible
    const modal = document.getElementById('logs-modal');
    if (!modal || modal.style.display === 'none') return;

    if (capturedLogs.length === 0) {
        container.innerHTML = '<p class="empty-message">No logs yet</p>';
        return;
    }

    container.innerHTML = '';
    capturedLogs.forEach(log => {
        const entry = document.createElement('div');
        entry.className = `log-entry ${log.level}`;

        const time = log.timestamp.toLocaleTimeString();
        const sourceLabel = log.source === 'worker' ? '[Worker]' : '[Main]';

        entry.innerHTML = `<span class="log-time">${time}</span><span class="log-source">${sourceLabel}</span>${escapeHtml(log.message)}`;
        container.appendChild(entry);
    });

    // Auto-scroll to bottom
    container.scrollTop = container.scrollHeight;
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Image Capture and Processing
function captureImageFromCamera(mode) {
    currentImageMode = mode;
    document.getElementById('camera-input').click();
}

function captureImageFromGallery(mode) {
    currentImageMode = mode;
    document.getElementById('gallery-input').click();
}

async function handleImageUpload(event) {
    const file = event.target.files[0];
    if (!file) return;

    if (!apiKey) {
        showError('Please add an API key in settings first');
        return;
    }

    // Create processing ID and add to queue
    const processingId = nextProcessingId++;
    processingQueue.push(processingId);

    const mode = currentImageMode;
    const modeLabel = mode === 'hand' ? 'Hand' : 'Table';

    showToast('Processing Started', `Analyzing ${modeLabel.toLowerCase()} image...`, 'info', 3000);

    // Convert image to base64
    const reader = new FileReader();
    reader.onload = async (e) => {
        const base64Image = e.target.result.split(',')[1];
        await processImageWithOpenAI(base64Image, mode, processingId, modeLabel);
    };
    reader.readAsDataURL(file);

    // Reset file input
    event.target.value = '';
}

// Default prompts for image analysis
const DEFAULT_HAND_PROMPT = `Analyze this Rummikub hand image and extract all tiles visible. For each tile, identify:
- The color: red (r), blue (b), yellow (y), or black (k)
- The number: 1-13
- Any wild/joker tiles (w)

IMPORTANT: If you see duplicate tiles (multiple tiles with the same color and number), you must include each one separately in the output. For example, if you see three red 5s, include "r5" three times in the tiles array.

Only extract tiles that are part of the player's hand. Disregard any tiles that may be laying on the table in the background - focus only on the hand tiles in the foreground.

Count carefully and report each physical tile you see exactly once.`;

const DEFAULT_TABLE_PROMPT = `Analyze this image showing Rummikub melds on a table. Extract all melds and identify:
- The meld type: either "run" (consecutive numbers, same color) or "group" (same number, different colors)
- For each meld, list the tiles with color and number

IMPORTANT: Some melds may be rotated (upside down or sideways). Read the tiles carefully regardless of orientation.
Process the melds from left to right, top to bottom as they appear in the image.

Only extract tiles that are already played on the table as complete melds. Disregard any tiles that are still part of someone's hand (not yet played on the table).

For each meld, determine whether it's a run or group, then list all tiles in that meld.`;

// Function schemas for OpenAI tool calling
const EXTRACT_HAND_TOOL = {
    type: "function",
    function: {
        name: "extract_hand_tiles",
        description: "Extract all Rummikub tiles from a hand image",
        parameters: {
            type: "object",
            properties: {
                tiles: {
                    type: "array",
                    description: "Array of tile strings. Each tile is represented as color+number (e.g., 'r5', 'b12', 'k1') or 'w' for wild. Include duplicates separately.",
                    items: {
                        type: "string",
                        pattern: "^([rbyk](1[0-3]|[1-9])|w)$"
                    }
                }
            },
            required: ["tiles"]
        }
    }
};

const EXTRACT_TABLE_TOOL = {
    type: "function",
    function: {
        name: "extract_table_melds",
        description: "Extract all Rummikub melds from a table image",
        parameters: {
            type: "object",
            properties: {
                melds: {
                    type: "array",
                    description: "Array of meld objects found on the table, ordered left to right, top to bottom",
                    items: {
                        type: "object",
                        properties: {
                            type: {
                                type: "string",
                                enum: ["run", "group"],
                                description: "Type of meld: 'run' for consecutive numbers with same color, 'group' for same number with different colors"
                            },
                            tiles: {
                                type: "array",
                                description: "Array of tile strings in this meld (e.g., ['y6', 'y7', 'y8'] or ['r5', 'b5', 'k5'])",
                                items: {
                                    type: "string",
                                    pattern: "^([rbyk](1[0-3]|[1-9])|w)$"
                                }
                            }
                        },
                        required: ["type", "tiles"]
                    }
                }
            },
            required: ["melds"]
        }
    }
};

async function processImageWithOpenAI(base64Image, mode, processingId, modeLabel) {
    try {
        const prompt = mode === 'hand' ? handPrompt : tablePrompt;
        const tool = mode === 'hand' ? EXTRACT_HAND_TOOL : EXTRACT_TABLE_TOOL;

        // Log request details (excluding base64 image for brevity)
        console.log(`[API Request] Model: ${modelName}, Mode: ${mode}`);
        console.log(`[API Request] Prompt: ${prompt}`);
        console.log(`[API Request] Tool: ${tool.function.name}`);

        const response = await fetch('https://openrouter.ai/api/v1/chat/completions', {
            method: 'POST',
            headers: {
                'Authorization': `Bearer ${apiKey}`,
                'Content-Type': 'application/json',
                'HTTP-Referer': window.location.href,
                'X-Title': 'Rummikub Solver'
            },
            body: JSON.stringify({
                model: modelName,
                messages: [
                    {
                        role: 'user',
                        content: [
                            {
                                type: 'image_url',
                                image_url: {
                                    url: `data:image/jpeg;base64,${base64Image}`
                                }
                            },
                            {
                                type: 'text',
                                text: prompt
                            }
                        ]
                    }
                ],
                tools: [tool],
                tool_choice: {
                    type: "function",
                    function: { name: tool.function.name }
                },
                max_tokens: 1024,
                temperature: 0.7
            })
        });

        if (!response.ok) {
            let errorMessage = `API Error (HTTP ${response.status})`;

            try {
                const errorData = await response.json();
                if (errorData.error?.message) {
                    errorMessage += `: ${errorData.error.message}`;
                } else if (errorData.message) {
                    errorMessage += `: ${errorData.message}`;
                }
            } catch (e) {
                // If response is not JSON, use status text
                errorMessage += `: ${response.statusText || 'Unknown error'}`;
            }

            if (response.status === 401) {
                errorMessage += ' - Please check your API key in settings.';
            }

            showToast(`${modeLabel} Processing Failed`, errorMessage, 'error', 8000);
            removeFromProcessingQueue(processingId);
            return;
        }

        const data = await response.json();

        // Log response message content and tool calls
        const messageContent = data.choices?.[0]?.message?.content;
        const toolCalls = data.choices?.[0]?.message?.tool_calls;
        console.log(`[API Response] Message content: ${messageContent || '(none)'}`);
        console.log(`[API Response] Tool calls:`, toolCalls || '(none)');

        if (!data.choices || data.choices.length === 0) {
            showToast(`${modeLabel} Processing Failed`, 'Invalid API response: No choices returned', 'error', 8000);
            removeFromProcessingQueue(processingId);
            return;
        }

        const toolCall = data.choices[0]?.message?.tool_calls?.[0];

        if (!toolCall || !toolCall.function?.arguments) {
            showToast(`${modeLabel} Processing Failed`, 'Invalid API response: No tool call or function arguments returned', 'error', 8000);
            removeFromProcessingQueue(processingId);
            return;
        }

        let result;
        try {
            result = JSON.parse(toolCall.function.arguments);
        } catch (e) {
            showToast(`${modeLabel} Processing Failed`, `Failed to parse function arguments - ${e.message}`, 'error', 8000);
            removeFromProcessingQueue(processingId);
            return;
        }

        if (mode === 'hand') {
            updateHandFromImage(result, processingId, modeLabel);
        } else {
            updateTableFromImage(result, processingId, modeLabel);
        }

    } catch (error) {
        console.error('Error processing image:', error);
        const errorType = error.name === 'TypeError' ? 'Network error' : 'Error';
        showToast(`${modeLabel} Processing Failed`, `${errorType}: ${error.message || 'Unknown error occurred'}`, 'error', 8000);
        removeFromProcessingQueue(processingId);
    }
}

// Remove processing ID from queue
function removeFromProcessingQueue(processingId) {
    const index = processingQueue.indexOf(processingId);
    if (index !== -1) {
        processingQueue.splice(index, 1);
    }
}

function updateHandFromImage(result, processingId, modeLabel) {
    if (!result.tiles || !Array.isArray(result.tiles)) {
        showToast(`${modeLabel} Processing Failed`, 'Invalid response format from API', 'error', 8000);
        removeFromProcessingQueue(processingId);
        return;
    }

    // Clear existing hand and add new tiles
    hand.clear();

    try {
        for (const tile of result.tiles) {
            if (!isValidTile(tile)) {
                throw new Error(`Invalid tile: ${tile}`);
            }
            const count = hand.get(tile) || 0;
            hand.set(tile, count + 1);
        }

        updateHandDisplay();
        updateTileCounts();
        showToast('Hand Updated', `Imported ${result.tiles.length} tiles from image`, 'success', 5000);
        removeFromProcessingQueue(processingId);
    } catch (error) {
        showToast(`${modeLabel} Processing Failed`, `Error updating hand: ${error.message}`, 'error', 8000);
        hand.clear();
        updateHandDisplay();
        removeFromProcessingQueue(processingId);
    }
}

function updateTableFromImage(result, processingId, modeLabel) {
    if (!result.melds || !Array.isArray(result.melds)) {
        showToast(`${modeLabel} Processing Failed`, 'Invalid response format from API', 'error', 8000);
        removeFromProcessingQueue(processingId);
        return;
    }

    try {
        let addedCount = 0;
        let duplicateCount = 0;

        // Process each meld
        for (const meld of result.melds) {
            if (!meld.type || !Array.isArray(meld.tiles)) {
                throw new Error('Invalid meld format');
            }

            // Validate all tiles
            for (const tile of meld.tiles) {
                if (!isValidTile(tile)) {
                    throw new Error(`Invalid tile: ${tile}`);
                }
            }

            if (meld.tiles.length < 3) {
                throw new Error(`Meld must have at least 3 tiles, got ${meld.tiles.length}`);
            }

            const meldObj = {
                type: meld.type,
                tiles: meld.tiles
            };

            // Check for duplicates
            const duplicateIndex = findDuplicateMeld(meldObj);
            if (duplicateIndex !== -1) {
                // Add to confirmation queue
                const tilesDisplay = meld.tiles.map(t => formatTileDisplay(t)).join(' ');
                addConfirmation(
                    meldObj,
                    `This ${meld.type} (${tilesDisplay}) matches an existing meld on the table.`
                );
                duplicateCount++;
            } else {
                // Add directly to table
                table.push(meldObj);
                addedCount++;
            }
        }

        updateTableDisplay();

        // Show appropriate toast
        if (addedCount > 0 && duplicateCount > 0) {
            showToast('Table Updated', `Added ${addedCount} meld(s), ${duplicateCount} duplicate(s) need confirmation`, 'success', 5000);
        } else if (addedCount > 0) {
            showToast('Table Updated', `Added ${addedCount} meld(s) to table`, 'success', 5000);
        } else if (duplicateCount > 0) {
            showToast('Duplicates Detected', `${duplicateCount} duplicate meld(s) need confirmation`, 'info', 5000);
        }

        removeFromProcessingQueue(processingId);
    } catch (error) {
        showToast(`${modeLabel} Processing Failed`, `Error updating table: ${error.message}`, 'error', 8000);
        updateTableDisplay();
        removeFromProcessingQueue(processingId);
    }
}

// Clear hand
function clearHand() {
    hand.clear();
    updateHandDisplay();
    updateTileCounts();
}

// Clear table
function clearTable() {
    table = [];
    updateTableDisplay();
}

// Attach event listeners
function attachEventListeners() {
    // Existing listeners
    document.getElementById('add-meld-btn').addEventListener('click', addMeldToTable);
    document.getElementById('solve-btn').addEventListener('click', solve);
    document.getElementById('save-btn').addEventListener('click', saveState);
    document.getElementById('clear-hand-btn').addEventListener('click', clearHand);
    document.getElementById('clear-table-btn').addEventListener('click', clearTable);

    // Menu
    document.getElementById('menu-btn').addEventListener('click', toggleMenu);
    document.getElementById('menu-settings').addEventListener('click', () => {
        closeMenu();
        openSettingsModal();
    });
    document.getElementById('menu-logs').addEventListener('click', openLogsModal);

    // Close menu when clicking outside
    document.addEventListener('click', (e) => {
        const menuContainer = document.getElementById('menu-btn').parentElement;
        if (menuOpen && !menuContainer.contains(e.target)) {
            closeMenu();
        }
    });

    // Settings modal
    document.getElementById('close-settings-btn').addEventListener('click', closeSettingsModal);
    document.getElementById('save-api-key-btn').addEventListener('click', saveApiKey);
    document.getElementById('restore-defaults-btn').addEventListener('click', restoreDefaultPrompts);

    // Logs modal
    document.getElementById('close-logs-btn').addEventListener('click', closeLogsModal);
    document.getElementById('clear-logs-btn').addEventListener('click', clearLogs);
    document.getElementById('logs-modal').addEventListener('click', (e) => {
        if (e.target.id === 'logs-modal') {
            closeLogsModal();
        }
    });

    // Image capture
    document.getElementById('camera-hand-btn').addEventListener('click', () => captureImageFromCamera('hand'));
    document.getElementById('gallery-hand-btn').addEventListener('click', () => captureImageFromGallery('hand'));
    document.getElementById('camera-table-btn').addEventListener('click', () => captureImageFromCamera('table'));
    document.getElementById('gallery-table-btn').addEventListener('click', () => captureImageFromGallery('table'));
    document.getElementById('camera-input').addEventListener('change', handleImageUpload);
    document.getElementById('gallery-input').addEventListener('change', handleImageUpload);

    // Close modal when clicking outside
    document.getElementById('settings-modal').addEventListener('click', (e) => {
        if (e.target.id === 'settings-modal') {
            closeSettingsModal();
        }
    });

    // Allow Enter key to add meld
    document.getElementById('meld-tiles').addEventListener('keypress', (e) => {
        if (e.key === 'Enter') {
            addMeldToTable();
        }
    });

    // Allow Enter key to save
    document.getElementById('save-name').addEventListener('keypress', (e) => {
        if (e.key === 'Enter') {
            saveState();
        }
    });

    // Allow Enter key in API key input
    document.getElementById('api-key-input').addEventListener('keypress', (e) => {
        if (e.key === 'Enter') {
            saveApiKey();
        }
    });

    // Allow Enter key in model input
    document.getElementById('model-input').addEventListener('keypress', (e) => {
        if (e.key === 'Enter') {
            saveApiKey();
        }
    });

    // Save time limit when changed
    document.getElementById('time-limit').addEventListener('change', saveTimeLimit);
}

// Make functions global for onclick handlers
window.removeTileFromHand = removeTileFromHand;
window.removeMeldFromTable = removeMeldFromTable;
window.deleteSavedState = deleteSavedState;
window.approveConfirmation = approveConfirmation;
window.rejectConfirmation = rejectConfirmation;

// Initialize app
initWasm().then(() => {
    initUI();
    initWorker(); // Initialize Web Worker for non-blocking solver
});
