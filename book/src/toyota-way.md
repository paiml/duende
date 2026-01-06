# Toyota Production System Principles

Duende is designed around Toyota Production System (TPS) principles.

## Jidoka (自働化) - Autonomation

**Stop on error, don't propagate defects.**

In duende:
- Daemons stop cleanly on fatal errors
- Health checks detect problems early
- Restart policies handle recovery

## Poka-Yoke (ポカヨケ) - Error Prevention

**Design systems to prevent errors.**

In duende:
- Configuration validation at load time
- Type-safe APIs prevent misuse
- Feature gates prevent platform mismatches

## Heijunka (平準化) - Load Leveling

**Smooth out workload variations.**

In duende:
- Resource limits prevent overload
- Backoff policies for restarts
- Queue management in daemon loops

## Muda (無駄) - Waste Elimination

**Eliminate unnecessary resource usage.**

In duende:
- Circuit breakers prevent wasted retries
- Memory limits prevent runaway allocation
- Efficient signal handling

## Kaizen (改善) - Continuous Improvement

**Measure, analyze, improve.**

In duende:
- RED metrics collection
- Health check scoring
- Observability integration

## Genchi Genbutsu (現地現物) - Go and See

**Direct observation of reality.**

In duende:
- `renacer` syscall tracing
- Process state monitoring
- Real-time metrics
