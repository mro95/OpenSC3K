// Create functions at every target of a detected vtable that auto-analysis missed, so the
// virtual methods can be matched and named. Vtables are found as in ExportFeatures.java:
// runs of pointers into executable memory, inside non-executable memory, whose start is
// referenced from code.
//
// Usage (headless post-script, project not read-only): FixupVtables.java
//@category SC3K

import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.Reference;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.TreeSet;

public class FixupVtables extends GhidraScript {
    private Memory mem;

    @Override
    protected void run() throws Exception {
        mem = currentProgram.getMemory();
        Set<Address> targets = new TreeSet<>();
        for (MemoryBlock b : mem.getBlocks()) {
            if (!b.isInitialized() || b.isExecute()) {
                continue;
            }
            long words = b.getSize() / 4;
            List<Address> run = new ArrayList<>();
            Address runStart = null;
            for (long i = 0; i <= words; i++) {
                Address p = i < words ? b.getStart().add(4 * i) : null;
                Address t = p != null ? pointerAt(p) : null;
                boolean code = t != null;
                if (!code && runStart != null) {
                    if (hasCodeRef(runStart)) {
                        targets.addAll(run);
                    }
                    run = new ArrayList<>();
                    runStart = null;
                }
                if (code) {
                    if (runStart == null) {
                        runStart = p;
                    }
                    run.add(t);
                }
            }
        }
        int created = 0;
        for (Address t : targets) {
            if (getFunctionAt(t) != null) {
                continue;
            }
            if (getInstructionAt(t) == null) {
                new DisassembleCommand(t, null, true).applyTo(currentProgram, monitor);
            }
            if (getInstructionAt(t) != null && getFunctionContaining(t) == null
                    && new CreateFunctionCmd(t).applyTo(currentProgram, monitor)) {
                created++;
            }
        }
        println(String.format("%s: %d vtable targets, %d functions created",
            currentProgram.getName(), targets.size(), created));
    }

    private Address pointerAt(Address p) {
        try {
            long v = mem.getInt(p) & 0xffffffffL;
            Address a = toAddr(v);
            MemoryBlock b = mem.getBlock(a);
            return v != 0 && b != null && b.isExecute() ? a : null;
        } catch (Exception e) {
            return null;
        }
    }

    private boolean hasCodeRef(Address a) {
        for (Reference r : getReferencesTo(a)) {
            if (getFunctionContaining(r.getFromAddress()) != null) {
                return true;
            }
        }
        return false;
    }
}
