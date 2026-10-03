// Export functions and RTTI-recovered classes of the current program as TSV.
// Usage (headless post-script): ExportSymbols.java <output.tsv>
//@category SC3K

import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Namespace;
import ghidra.program.model.symbol.SourceType;
import ghidra.program.model.symbol.Symbol;
import ghidra.program.model.symbol.SymbolType;

import java.io.PrintWriter;

public class ExportSymbols extends GhidraScript {
    @Override
    protected void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length != 1) {
            throw new IllegalArgumentException("usage: ExportSymbols.java <output.tsv>");
        }
        long base = currentProgram.getImageBase().getOffset();
        int functions = 0, named = 0, classes = 0;
        try (PrintWriter out = new PrintWriter(args[0], "UTF-8")) {
            out.println("# " + currentProgram.getName() + " imagebase=0x" + Long.toHexString(base));
            out.println("kind\trva\tname\tsource\tsignature");
            for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
                if (f.isExternal() || f.isThunk()) {
                    continue;
                }
                long rva = f.getEntryPoint().getOffset() - base;
                SourceType src = f.getSymbol().getSource();
                out.printf("func\t0x%06x\t%s\t%s\t%s%n", rva, f.getName(true), src,
                    f.getSignature().getPrototypeString());
                functions++;
                // Unwind@/Catch@ are compiler-generated EH funclets, not real names.
                String name = f.getName();
                if (src != SourceType.DEFAULT && !name.startsWith("Unwind@") && !name.startsWith("Catch@")) {
                    named++;
                }
            }
            for (Symbol s : currentProgram.getSymbolTable().getAllSymbols(true)) {
                if (s.getSymbolType() == SymbolType.CLASS) {
                    Namespace ns = (Namespace) s.getObject();
                    out.printf("class\t\t%s\t%s\t%n", ns.getName(true), s.getSource());
                    classes++;
                }
            }
        }
        println(String.format("%s: %d functions (%d named), %d classes -> %s",
            currentProgram.getName(), functions, named, classes, args[0]));
    }
}
