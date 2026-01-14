// State management
let hand = new Map(); // tile -> count
let table = []; // array of meld objects
let wasmModule = null;

// Initialize WASM
async function initWasm() {
    try {
        const wasm = await import('./pkg/rummikub_solver.js');
        await wasm.default();
        wasmModule = wasm;
        console.log('WASM module loaded successfully');
    } catch (error) {
        console.error('Failed to load WASM:', error);
        showError('Failed to load solver module. Make sure WASM files are built.');
    }
}

// Initialize UI
function initUI() {
    createTilePicker();
    updateHandDisplay();
    updateTableDisplay();
    loadSavedStates();
    attachEventListeners();
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
        return a[0].localeCompare(b[0]);
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

    // Parse tiles (space-separated)
    const tiles = tilesStr.toLowerCase().split(/\s+/);

    // Validate tiles
    for (const tile of tiles) {
        if (!isValidTile(tile)) {
            showError(`Invalid tile: ${tile}`);
            return;
        }
    }

    if (tiles.length < 3) {
        showError('A meld must have at least 3 tiles');
        return;
    }

    const meld = { type, tiles };
    table.push(meld);

    updateTableDisplay();
    tilesInput.value = '';
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

// Solve the game
async function solve() {
    if (!wasmModule) {
        showError('WASM module not loaded yet');
        return;
    }

    // Get total tiles in hand
    let totalTiles = 0;
    hand.forEach(count => totalTiles += count);

    if (totalTiles === 0) {
        showError('Please add tiles to your hand');
        return;
    }

    const strategy = document.getElementById('strategy').value;
    const timeLimit = parseInt(document.getElementById('time-limit').value);

    const solveBtn = document.getElementById('solve-btn');
    solveBtn.disabled = true;
    solveBtn.textContent = 'Solving';
    solveBtn.classList.add('loading');

    try {
        // Convert hand to array format
        const handArray = [];
        hand.forEach((count, tile) => {
            for (let i = 0; i < count; i++) {
                handArray.push(tile);
            }
        });

        // Call WASM solver
        const resultJson = wasmModule.solve_rummikub(
            JSON.stringify(handArray),
            JSON.stringify(table),
            strategy,
            BigInt(timeLimit)
        );

        const result = JSON.parse(resultJson);

        displayResults(result);

    } catch (error) {
        console.error('Solver error:', error);
        showError('Solver error: ' + error.message);
    } finally {
        solveBtn.disabled = false;
        solveBtn.textContent = 'Find Best Moves';
        solveBtn.classList.remove('loading');
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

    const moves = result.moves || [];

    if (moves.length === 0) {
        display.innerHTML = `
            <div class="result-success">
                No moves needed - your hand is already optimal!
            </div>
        `;
        return;
    }

    let html = `
        <div class="result-success">
            Found solution with ${moves.length} move${moves.length > 1 ? 's' : ''}!
        </div>
        <ol class="move-list">
    `;

    moves.forEach((move, index) => {
        html += `<li class="move-item">`;
        html += `<span class="move-number">${index + 1}.</span>`;

        if (move.action === 'pickup') {
            html += `Pick up meld #${move.index + 1} from the table`;
        } else if (move.action === 'laydown') {
            const meld = move.meld;
            const tiles = meld.tiles.map(t => formatTileDisplay(t)).join(' ');
            html += `Lay down ${meld.type}: ${tiles}`;
        }

        html += `</li>`;
    });

    html += '</ol>';
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

    setTimeout(() => {
        section.style.display = 'none';
    }, 5000);
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

    showSuccess('State saved successfully!');
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

    showSuccess('State loaded successfully!');
}

// Delete a saved state
function deleteSavedState(index) {
    const states = getSavedStates();
    states.splice(index, 1);
    localStorage.setItem('rummikub-saved-states', JSON.stringify(states));
    loadSavedStates();
}

// Show success message
function showSuccess(message) {
    const section = document.getElementById('results-section');
    const display = document.getElementById('results-display');

    section.style.display = 'block';
    display.innerHTML = `<div class="result-success">${message}</div>`;

    setTimeout(() => {
        section.style.display = 'none';
    }, 3000);
}

// Clear hand
function clearHand() {
    hand.clear();
    updateHandDisplay();
    updateTileCounts();
}

// Attach event listeners
function attachEventListeners() {
    document.getElementById('add-meld-btn').addEventListener('click', addMeldToTable);
    document.getElementById('solve-btn').addEventListener('click', solve);
    document.getElementById('save-btn').addEventListener('click', saveState);
    document.getElementById('clear-hand-btn').addEventListener('click', clearHand);

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
}

// Make functions global for onclick handlers
window.removeTileFromHand = removeTileFromHand;
window.removeMeldFromTable = removeMeldFromTable;
window.deleteSavedState = deleteSavedState;

// Initialize app
initWasm().then(() => {
    initUI();
});
