#!/bin/bash
# Check that WebView is not used in core UI (atom-ui crate)
# According to CLAUDE.md: WebView is only allowed in isolated VS Code extension webview API

set -euo pipefail

echo "🔍 Checking for WebView usage in core UI..."

# Exit code tracking
EXIT_CODE=0

# Check atom-ui crate specifically (core UI must not use WebView)
if grep -r -i "webview" crates/atom-ui/ --include="*.rs" --include="*.toml" 2>/dev/null; then
    echo "❌ ERROR: Found WebView usage in core UI (crates/atom-ui/)"
    echo "   WebView is only allowed in isolated VS Code extension API"
    EXIT_CODE=1
fi

# Check for WebView-related dependencies in core crates
CORE_CRATES=("atom-ui" "atom-core" "atomd")
for crate in "${CORE_CRATES[@]}"; do
    if [ -f "crates/${crate}/Cargo.toml" ] || [ -f "apps/${crate}/Cargo.toml" ]; then
        CARGO_FILE=""
        if [ -f "crates/${crate}/Cargo.toml" ]; then
            CARGO_FILE="crates/${crate}/Cargo.toml"
        else
            CARGO_FILE="apps/${crate}/Cargo.toml"
        fi
        
        if grep -i -E "(webview|webkit|webengine|chromium)" "$CARGO_FILE" 2>/dev/null; then
            echo "❌ ERROR: Found WebView dependency in ${crate}: $CARGO_FILE"
            EXIT_CODE=1
        fi
    fi
done

# Check dependency tree for webview-related crates (requires cargo to be available)
if command -v cargo >/dev/null 2>&1; then
    echo "🔍 Checking dependency tree for WebView crates..."
    
    # List of prohibited WebView-related crate names
    WEBVIEW_CRATES=("webview" "webkit2gtk" "webkitgtk" "wry" "tao" "webview2" "webview2-com")
    
    for webview_crate in "${WEBVIEW_CRATES[@]}"; do
        if cargo tree 2>/dev/null | grep -q "$webview_crate"; then
            echo "❌ ERROR: Found prohibited WebView crate in dependency tree: $webview_crate"
            EXIT_CODE=1
        fi
    done
else
    echo "⚠️  WARNING: cargo not available, skipping dependency tree check"
fi

if [ $EXIT_CODE -eq 0 ]; then
    echo "✅ No WebView usage found in core UI"
else
    echo ""
    echo "Policy violation: WebView is prohibited in core UI according to CLAUDE.md"
    echo "WebView is only allowed in isolated VS Code extension webview API for compatibility"
fi

exit $EXIT_CODE