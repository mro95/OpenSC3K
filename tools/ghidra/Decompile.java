// Decompile selected functions to a text file. A selector is a hex address (0x0820bc04,
// the function containing it), @ plus a hex address (@0x4faa28, every function that
// references it) or a substring of the fully qualified function name (cSC3MainMenu::).
//
// Usage (headless post-script): Decompile.java <out.c> <selector>...
//@category SC3K

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileOptions;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;

import java.io.PrintWriter;
import java.util.Arrays;
import java.util.LinkedHashSet;
import java.util.Set;

public class Decompile extends GhidraScript {
    @Override
    protected void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 2) {
            throw new IllegalArgumentException("usage: Decompile.java <out.c> <selector>...");
        }
        Set<Function> funcs = new LinkedHashSet<>();
        for (String sel : Arrays.copyOfRange(args, 1, args.length)) {
            if (sel.startsWith("@0x")) {
                for (Reference r : getReferencesTo(toAddr(Long.parseLong(sel.substring(3), 16)))) {
                    Function f = getFunctionContaining(r.getFromAddress());
                    if (f != null) {
                        funcs.add(f);
                    }
                }
                continue;
            }
            if (sel.startsWith("0x")) {
                Address a = currentProgram.getAddressFactory().getDefaultAddressSpace()
                    .getAddress(Long.parseLong(sel.substring(2), 16));
                Function f = getFunctionContaining(a);
                if (f != null) {
                    funcs.add(f);
                }
                continue;
            }
            for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
                if (f.getName(true).contains(sel)) {
                    funcs.add(f);
                }
            }
        }
        DecompInterface di = new DecompInterface();
        DecompileOptions opts = new DecompileOptions();
        opts.grabFromProgram(currentProgram);
        di.setOptions(opts);
        di.openProgram(currentProgram);
        try (PrintWriter out = new PrintWriter(args[0])) {
            for (Function f : funcs) {
                DecompileResults r = di.decompileFunction(f, 120, monitor);
                out.printf("// %s @ %s%n", f.getName(true), f.getEntryPoint());
                if (r.decompileCompleted()) {
                    out.println(r.getDecompiledFunction().getC());
                } else {
                    out.println("// decompile failed: " + r.getErrorMessage());
                }
            }
        }
        di.dispose();
        println(String.format("%s: decompiled %d functions to %s", currentProgram.getName(),
            funcs.size(), args[0]));
    }
}
