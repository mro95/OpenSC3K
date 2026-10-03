// Undo bogus "non-returning" marks. In the Loki libraries Ghidra's analysis marks ordinary
// functions (free, __builtin_new, pthread_mutex_lock, RZ::WCM, ...) as non-returning, so
// every caller is cut off after the first call to them and decompiles truncated. This clears
// the flag on everything except genuine non-returning functions, disassembles the code after
// each affected call, and regrows the containing function bodies.
//
// Usage (headless post-script, project not read-only): FixNoReturn.java
//@category SC3K

import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.symbol.FlowType;
import ghidra.program.model.symbol.Reference;

import java.util.HashSet;
import java.util.Set;
import java.util.regex.Pattern;

public class FixNoReturn extends GhidraScript {
    private static final Pattern GENUINE =
        Pattern.compile("(?i).*(exit|abort|terminate|throw|assert_fail|longjmp|panic).*");

    @Override
    protected void run() throws Exception {
        Set<Function> cleared = new HashSet<>();
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            clear(f, cleared);
        }
        for (Function f : currentProgram.getFunctionManager().getExternalFunctions()) {
            clear(f, cleared);
        }
        int resumed = 0;
        Set<Function> touched = new HashSet<>();
        for (Function f : cleared) {
            for (Reference r : getReferencesTo(f.getEntryPoint())) {
                resumed += resume(r, touched);
            }
            // Calls through PLT thunks reference the thunk, not the target.
            for (Address thunk : f.getFunctionThunkAddresses(true) == null ? new Address[0]
                    : f.getFunctionThunkAddresses(true)) {
                for (Reference r : getReferencesTo(thunk)) {
                    resumed += resume(r, touched);
                }
            }
        }
        for (Function f : touched) {
            CreateFunctionCmd.fixupFunctionBody(currentProgram, f, monitor);
        }
        println(String.format("%s: cleared %d non-returning marks, resumed %d call sites, regrew %d functions",
            currentProgram.getName(), cleared.size(), resumed, touched.size()));
    }

    private void clear(Function f, Set<Function> cleared) {
        if (f.hasNoReturn() && !GENUINE.matcher(f.getName()).matches()) {
            f.setNoReturn(false);
            cleared.add(f);
        }
    }

    private int resume(Reference r, Set<Function> touched) {
        if (!r.getReferenceType().isCall()) {
            return 0;
        }
        Instruction call = getInstructionAt(r.getFromAddress());
        if (call == null) {
            return 0;
        }
        if (call.getFlowOverride() != null && call.getFlowType() != FlowType.UNCONDITIONAL_CALL) {
            call.setFlowOverride(ghidra.program.model.listing.FlowOverride.NONE);
        }
        Address next = call.getDefaultFallThrough();
        if (next == null) {
            return 0;
        }
        Function caller = getFunctionContaining(call.getAddress());
        if (caller != null) {
            touched.add(caller);
        }
        if (getInstructionAt(next) != null) {
            return 0;
        }
        new DisassembleCommand(next, null, true).applyTo(currentProgram, monitor);
        return 1;
    }
}
