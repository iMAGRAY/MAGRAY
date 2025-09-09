#!/bin/bash
# Check that Visual Studio Marketplace is not used in the project
# According to CLAUDE.md: Only Open VSX is allowed due to VS Marketplace ToU restrictions

set -euo pipefail

echo "🔍 Checking for Visual Studio Marketplace usage..."

# Exit code tracking: 0 - success, 1 - violation found
EXIT_CODE=0

# Prohibited marketplace patterns
MARKETPLACE_PATTERNS=(
    "marketplace.visualstudio.com"
    "vscode.dev/marketplace"
    "ms-vscode-marketplace"
    "visual-studio-marketplace"
)

echo "🔍 Scanning source code for marketplace patterns..."

# Check all source files, configs, and docs
for pattern in "${MARKETPLACE_PATTERNS[@]}"; do
    echo "  Checking pattern: $pattern"
    
    if grep -r -i "$pattern" . \
        --include="*.rs" \
        --include="*.toml" \
        --include="*.json" \
        --include="*.md" \
        --include="*.yml" \
        --include="*.yaml" \
        --include="*.js" \
        --include="*.ts" \
        --exclude-dir=target \
        --exclude-dir=node_modules \
        --exclude-dir=.git \
        --exclude="no-mock.md" \
        2>/dev/null; then
        
        echo "❌ ERROR: Found Visual Studio Marketplace reference: $pattern"
        echo "   Only Open VSX is allowed according to CLAUDE.md ToU policy"
        EXIT_CODE=1
    fi
done

# Check for specific configuration files that might contain marketplace URLs
CONFIG_FILES=(
    "product.json"
    "extensions.json"
    ".vscode/extensions.json"
    "package.json"
)

echo "🔍 Checking configuration files..."

for config_file in "${CONFIG_FILES[@]}"; do
    if [ -f "$config_file" ]; then
        echo "  Checking: $config_file"
        
        # Check for marketplace URLs in JSON configs
        if grep -i -E "(marketplace\.visualstudio\.com|\"serviceUrl\".*marketplace)" "$config_file" 2>/dev/null; then
            echo "❌ ERROR: Found marketplace URL in $config_file"
            echo "   Use Open VSX serviceUrl instead: https://open-vsx.org/vscode/gallery"
            EXIT_CODE=1
        fi
    fi
done

# Check for correct Open VSX configuration
echo "🔍 Verifying Open VSX configuration..."

EXPECTED_PATTERNS=(
    "open-vsx.org"
    "openvsx.org"
)

FOUND_OPENVSX=false
for pattern in "${EXPECTED_PATTERNS[@]}"; do
    if grep -r -i "$pattern" . \
        --include="*.json" \
        --include="*.toml" \
        --exclude-dir=target \
        --exclude-dir=.git \
        2>/dev/null >/dev/null; then
        FOUND_OPENVSX=true
        echo "✅ Found Open VSX configuration: $pattern"
        break
    fi
done

# Verify product.json has correct Open VSX URLs if it exists
if [ -f "product.json" ]; then
    echo "🔍 Validating product.json Open VSX configuration..."
    
    if ! grep -q "open-vsx.org" "product.json" 2>/dev/null; then
        echo "❌ ERROR: product.json exists but doesn't contain Open VSX URLs"
        echo "   Expected serviceUrl: https://open-vsx.org/vscode/gallery"
        echo "   Expected itemUrl: https://open-vsx.org/vscode/item"
        EXIT_CODE=1
    else
        echo "✅ product.json contains correct Open VSX configuration"
    fi
fi

# Check package manager lock files for marketplace-related packages
if [ -f "package-lock.json" ] || [ -f "yarn.lock" ] || [ -f "pnpm-lock.yaml" ]; then
    echo "🔍 Checking package manager lock files..."
    
    LOCKFILES=("package-lock.json" "yarn.lock" "pnpm-lock.yaml")
    
    for lockfile in "${LOCKFILES[@]}"; do
        if [ -f "$lockfile" ]; then
            if grep -i "marketplace" "$lockfile" 2>/dev/null; then
                echo "❌ ERROR: Found marketplace reference in $lockfile"
                EXIT_CODE=1
            fi
        fi
    done
fi

# Summary
if [ $EXIT_CODE -eq 0 ]; then
    echo "✅ No Visual Studio Marketplace usage found"
    if [ "$FOUND_OPENVSX" = true ]; then
        echo "✅ Open VSX configuration detected"
    else
        echo "ℹ️  No extension registry configuration found (acceptable for current stage)"
    fi
else
    echo ""
    echo "Policy violation: Visual Studio Marketplace usage is prohibited"
    echo "Reason: VS Marketplace ToU restricts usage to 'In-Scope Products and Services'"
    echo "Solution: Use Open VSX registry instead (https://open-vsx.org/)"
    echo ""
    echo "Correct configuration example:"
    echo '{'
    echo '  "extensionsGallery": {'
    echo '    "serviceUrl": "https://open-vsx.org/vscode/gallery",'
    echo '    "itemUrl": "https://open-vsx.org/vscode/item"'
    echo '  }'
    echo '}'
fi

exit $EXIT_CODE