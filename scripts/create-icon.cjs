const fs = require('fs');
const path = require('path');

// Minimal valid 1x1 RGBA PNG (blue pixel) in base64
const minimalPNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64'
);

// For now, just use this minimal PNG - Tauri will handle resizing
const iconPath = path.join(__dirname, '..', 'src-tauri', 'icons', 'icon.png');
fs.writeFileSync(iconPath, minimalPNG);
console.log('Created minimal icon.png - you should replace this with a proper 1024x1024 icon later');
