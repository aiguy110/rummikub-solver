// State management
let hand = new Map(); // tile -> count
let table = []; // array of meld objects
let wasmModule = null;
let apiKey = null;
let currentImageMode = null; // 'hand' or 'table'

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
    loadApiKey();
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

// Parse group meld: "5 r b k" format (number followed by colors)
function parseGroupTiles(input) {
    const parts = input.toLowerCase().split(/\s+/).filter(p => p);

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
        tiles.push(`${color}${number}`);
    }

    return tiles;
}

// Parse run meld: "y 6 7 8" format (color followed by numbers)
function parseRunTiles(input) {
    const parts = input.toLowerCase().split(/\s+/).filter(p => p);

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
        const num = parseInt(numStr);
        if (isNaN(num) || num < 1 || num > 13) {
            throw new Error(`Invalid number: ${numStr}. Must be 1-13.`);
        }
        tiles.push(`${color}${num}`);
    }

    return tiles;
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

// API Key Management
function loadApiKey() {
    apiKey = localStorage.getItem('rummikub-openai-api-key');
    updateCaptureButtonVisibility();
}

function saveApiKey() {
    const input = document.getElementById('api-key-input');
    const key = input.value.trim();

    if (!key) {
        showError('Please enter an API key');
        return;
    }

    if (!key.startsWith('sk-')) {
        showError('API key should start with "sk-"');
        return;
    }

    localStorage.setItem('rummikub-openai-api-key', key);
    apiKey = key;
    input.value = '';

    document.getElementById('settings-modal').style.display = 'none';
    updateCaptureButtonVisibility();
    showSuccess('API key saved successfully!');
}

function updateCaptureButtonVisibility() {
    const handBtn = document.getElementById('capture-hand-btn');
    const tableBtn = document.getElementById('capture-table-btn');

    if (apiKey) {
        handBtn.style.display = 'inline-block';
        tableBtn.style.display = 'inline-block';
    } else {
        handBtn.style.display = 'none';
        tableBtn.style.display = 'none';
    }
}

// Settings Modal Management
function openSettingsModal() {
    const modal = document.getElementById('settings-modal');
    const input = document.getElementById('api-key-input');

    if (apiKey) {
        input.value = apiKey;
    }

    modal.style.display = 'flex';
}

function closeSettingsModal() {
    document.getElementById('settings-modal').style.display = 'none';
}

// Image Capture and Processing
function captureImage(mode) {
    currentImageMode = mode;
    document.getElementById('image-input').click();
}

async function handleImageUpload(event) {
    const file = event.target.files[0];
    if (!file) return;

    if (!apiKey) {
        showError('Please add an OpenAI API key in settings first');
        return;
    }

    // Convert image to base64
    const reader = new FileReader();
    reader.onload = async (e) => {
        const base64Image = e.target.result.split(',')[1];
        await processImageWithOpenAI(base64Image, currentImageMode);
    };
    reader.readAsDataURL(file);

    // Reset file input
    event.target.value = '';
}

async function processImageWithOpenAI(base64Image, mode) {
    const btn = currentImageMode === 'hand'
        ? document.getElementById('capture-hand-btn')
        : document.getElementById('capture-table-btn');

    const originalText = btn.textContent;
    btn.disabled = true;
    btn.textContent = '⏳ Processing...';

    try {
        const prompt = mode === 'hand'
            ? `Analyze this Rummikub hand image and extract all tiles visible. For each tile, identify:
- The color: red (r), blue (b), yellow (y), or black (k)
- The number: 1-13
- Any wild/joker tiles (w)

Return the result as a JSON object with a "tiles" array, where each tile is represented as a string (e.g., "r5", "b12", "k1", "w" for wild).
Example: {"tiles": ["r1", "r2", "b5", "y10", "w"]}`
            : `Analyze this image showing Rummikub melds on a table. Extract all melds and identify:
- The meld type: either "run" (consecutive numbers, same color) or "group" (same number, different colors)
- For each meld, list the tiles with color and number

Return the result as a JSON object with a "melds" array. Each meld object should have:
- "type": "run" or "group"
- "tiles": array of tile strings (e.g., ["y6", "y7", "y8"] for a yellow run, or ["r5", "b5", "k5"] for a group)

Example: {"melds": [{"type": "run", "tiles": ["r1", "r2", "r3"]}, {"type": "group", "tiles": ["b7", "y7", "k7"]}]}`;

        const response = await fetch('https://api.openai.com/v1/chat/completions', {
            method: 'POST',
            headers: {
                'Authorization': `Bearer ${apiKey}`,
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                model: 'gpt-4o',
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
                max_tokens: 1024,
                temperature: 0.7
            })
        });

        if (!response.ok) {
            const errorData = await response.json();
            if (response.status === 401) {
                showError('Invalid API key. Please check your OpenAI API key in settings.');
                localStorage.removeItem('rummikub-openai-api-key');
                apiKey = null;
                updateCaptureButtonVisibility();
            } else {
                showError(`OpenAI API error: ${errorData.error?.message || 'Unknown error'}`);
            }
            return;
        }

        const data = await response.json();
        const content = data.choices[0]?.message?.content;

        if (!content) {
            showError('No response from OpenAI');
            return;
        }

        // Extract JSON from response
        const jsonMatch = content.match(/\{[\s\S]*\}/);
        if (!jsonMatch) {
            showError('Could not parse response from OpenAI');
            return;
        }

        const result = JSON.parse(jsonMatch[0]);

        if (mode === 'hand') {
            updateHandFromImage(result);
        } else {
            updateTableFromImage(result);
        }

    } catch (error) {
        console.error('Error processing image:', error);
        showError(`Error: ${error.message}`);
    } finally {
        btn.disabled = false;
        btn.textContent = originalText;
    }
}

function updateHandFromImage(result) {
    if (!result.tiles || !Array.isArray(result.tiles)) {
        showError('Invalid response format from OpenAI');
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
        showSuccess(`Imported ${result.tiles.length} tiles from image!`);
    } catch (error) {
        showError(`Error updating hand: ${error.message}`);
        hand.clear();
        updateHandDisplay();
    }
}

function updateTableFromImage(result) {
    if (!result.melds || !Array.isArray(result.melds)) {
        showError('Invalid response format from OpenAI');
        return;
    }

    try {
        // Clear existing table and add new melds
        table = [];

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

            table.push({
                type: meld.type,
                tiles: meld.tiles
            });
        }

        updateTableDisplay();
        showSuccess(`Imported ${result.melds.length} melds from image!`);
    } catch (error) {
        showError(`Error updating table: ${error.message}`);
        table = [];
        updateTableDisplay();
    }
}

// Clear hand
function clearHand() {
    hand.clear();
    updateHandDisplay();
    updateTileCounts();
}

// Attach event listeners
function attachEventListeners() {
    // Existing listeners
    document.getElementById('add-meld-btn').addEventListener('click', addMeldToTable);
    document.getElementById('solve-btn').addEventListener('click', solve);
    document.getElementById('save-btn').addEventListener('click', saveState);
    document.getElementById('clear-hand-btn').addEventListener('click', clearHand);

    // Settings modal
    document.getElementById('settings-btn').addEventListener('click', openSettingsModal);
    document.getElementById('close-settings-btn').addEventListener('click', closeSettingsModal);
    document.getElementById('save-api-key-btn').addEventListener('click', saveApiKey);

    // Image capture
    document.getElementById('capture-hand-btn').addEventListener('click', () => captureImage('hand'));
    document.getElementById('capture-table-btn').addEventListener('click', () => captureImage('table'));
    document.getElementById('image-input').addEventListener('change', handleImageUpload);

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
}

// Make functions global for onclick handlers
window.removeTileFromHand = removeTileFromHand;
window.removeMeldFromTable = removeMeldFromTable;
window.deleteSavedState = deleteSavedState;
window.captureImage = captureImage;

// Initialize app
initWasm().then(() => {
    initUI();
});
