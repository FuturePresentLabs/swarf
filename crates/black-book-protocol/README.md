# Black Book public contract

Small, independently licensed Rust/Serde types for optional engineering providers.
No formulas, material tables, machine defaults or Black Book source are included.
Black Book's implementation is proprietary; this crate's MIT/Apache license
applies only to the public contract.

One JSON request, followed by EOF, produces one JSON response. Payloads are at
most 4096 bytes. Unknown schemas, fields and invalid quantities fail with a
nonzero exit and diagnostic on stderr; stdout remains empty on failure. Request
types deny unknown fields. Response types retain the provider's version.

Supported v1 contracts:

| Schema | Inputs | Outputs |
| --- | --- | --- |
| `black-book.evaluate.v1` | mm3/min removal rate; N/mm2 cutting-force range; RPM; mm diameter; J/K capacitance; W/K conductance; Celsius temperatures; W heat; seconds | W power, N mean tangential force, Nm torque, bulk Celsius temperature |
| `black-book.four-axis.v1` | Three linear joints and one rotary in declared serial-chain order; home transform; normalized space screw axes; joint values and limits | Pose, local Z direction in world, 6x4 space Jacobian |

Four-axis translation, rotary offsets and prismatic joint values use **metres**;
rotary values use **radians**. Quaternion order is **w,x,y,z**. Screw and Jacobian
rows use **omega_x,y,z,v_x,y,z**. Joint columns retain the supplied chain order.
Limits reject out-of-range joints; they do not clamp. The model maps the declared
end-effector frame into the declared world frame. The home transform must include
the intended TCP offset. A rotary table requires the caller to declare the
workpiece/tool frame relationship; this contract does not guess it.

The included XYZ+A fixture is a synthetic serial chain, not the shop's measured
machine geometry. It rotates the home tool frame about X. At 90 degrees its home
100 mm Z offset rotates to negative Y; its joint translations remain in world.
An independently computed expected response is covered by owner integration tests.

The optional `Evaluator` trait supports direct linked calls. It contains no
implementation. FOSS callers can use a precompiled executable instead; they
must enforce payload bounds, deadlines and provider provenance. Merely depending
on this crate does not start a provider or require the proprietary checkout.

## Compatibility

Unknown fields are rejected: adding required fields or changing units requires a
new schema version. Provider implementation versions may change independently.
Compare JSON floating-point results numerically, rather than requiring identical
decimal formatting. Version 0.1.0 exposes forward kinematics only: controller
posts, inverse kinematics, singularity policy, collision checks and machining
qualification are outside this contract.

```sh
cargo test --manifest-path crates/black-book-protocol/Cargo.toml
cargo package --manifest-path crates/black-book-protocol/Cargo.toml
```
