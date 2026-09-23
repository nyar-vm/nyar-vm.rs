import * as fs from 'fs';
import * as path from 'path';

/**
 * 检查 Rust 项目中超过 1000 行的文件并给出重构建议
 */

const PROJECT_ROOT = path.resolve(__dirname, '..');
const MAX_LINES = 1000;
const IGNORE_DIRS = ['target', 'node_modules', '.git', '.github', '.run', 'documentation'];

function getRsFiles(dir: string, fileList: string[] = []): string[] {
    try {
        const files = fs.readdirSync(dir);
        for (const file of files) {
            const filePath = path.join(dir, file);
            if (IGNORE_DIRS.includes(file)) continue;

            const stat = fs.statSync(filePath);
            if (stat.isDirectory()) {
                getRsFiles(filePath, fileList);
            } else if (filePath.endsWith('.rs')) {
                fileList.push(filePath);
            }
        }
    } catch (err) {
        console.error(`Error reading directory ${dir}: ${err}`);
    }
    return fileList;
}

function analyzeFile(filePath: string) {
    const content = fs.readFileSync(filePath, 'utf-8');
    const lines = content.split('\n');
    const lineCount = lines.length;

    if (lineCount > MAX_LINES) {
        const relativePath = path.relative(PROJECT_ROOT, filePath);
        console.log(`\x1b[33m[Large File Found]\x1b[0m`);
        console.log(`Path: ${relativePath}`);
        console.log(`Lines: ${lineCount}`);
        console.log(`\x1b[36mRefactoring Suggestions:\x1b[0m`);

        // 基础建议
        console.log(
            `- Consider splitting this file into multiple sub-modules (e.g., move logic to a sub-directory with mod.rs).`,
        );

        // 检查 impl 块
        const implMatches = content.match(/impl\s+[\w<>, ]+\s+for\s+[\w<>, ]+\s*\{/g) || [];
        const structImplMatches = content.match(/impl\s+[\w<>, ]+\s*\{/g) || [];
        if (implMatches.length + structImplMatches.length > 3) {
            console.log(
                `- Found ${implMatches.length + structImplMatches.length} 'impl' blocks. Consider extracting each major 'impl' into its own file.`,
            );
        }

        // 检查函数长度
        let maxFuncLines = 0;
        let currentFuncLines = 0;
        let inFunc = false;
        for (const line of lines) {
            if (line.trim().startsWith('fn ') || line.trim().startsWith('pub fn ')) {
                inFunc = true;
                currentFuncLines = 0;
            }
            if (inFunc) {
                currentFuncLines++;
                if (line.trim() === '}') {
                    if (currentFuncLines > maxFuncLines) maxFuncLines = currentFuncLines;
                    inFunc = false;
                }
            }
        }
        if (maxFuncLines > 100) {
            console.log(
                `- Detected functions with over ${maxFuncLines} lines. Break down complex functions into smaller, reusable helpers.`,
            );
        }

        // 检查宏使用
        if (content.includes('macro_rules!')) {
            console.log(
                `- Contains macro definitions. If macros are large, move them to a dedicated 'macros.rs' file.`,
            );
        }

        console.log(`--------------------------------------------------\n`);
    }
}

function main() {
    console.log(`\x1b[1mChecking for Rust files with more than ${MAX_LINES} lines in:\x1b[0m`);
    console.log(`${PROJECT_ROOT}\n`);

    const files = getRsFiles(PROJECT_ROOT);
    let largeFileCount = 0;

    for (const file of files) {
        if (fs.readFileSync(file, 'utf-8').split('\n').length > MAX_LINES) {
            analyzeFile(file);
            largeFileCount++;
        }
    }

    if (largeFileCount === 0) {
        console.log('\x1b[32mGreat! No files exceeding 1000 lines were found.\x1b[0m');
    } else {
        console.log(`\x1b[1mTotal large files found: ${largeFileCount}\x1b[0m`);
    }
}

main();
