## Symbolic execution for the QBE IL

qsym is a [symbolic execution][symex wikipedia] tool for the [QBE][qbe web] intermediate language.
The tool leverages [Z3][z3 web] to execute QBE IL based on [SMT bitvectors][smt wikipedia].
This enables qsym to reason about conditional jumps in the QBE IL, exploring both branches (if feasible under the current constraints).

### Status

qsym is in very early stages of development and presently mostly a proof-of-concept.
The underlying parser for the QBE IL ([qbe-reader][qbe-reader github]) is also not yet complete, hence it does not support every syntactically valid QBE IL input yet.
Furthermore, it is assumed that input programs are well typed, e.g. no type checks are performed for instruction arguments.

### Installation

Clone the repository and run the following command:

    $ cargo install --path .

### Usage Example

Presently, qsym treats the parameters of a selected function as unconstrained symbolic and executes this function.
Consider the following example:

    $ cat input.qbe
    function w $main(w %a) {
    @start
            %a =w add 0, %a
            jnz %a, @end1, @end2
    @end1
            %exit =w add 0, 1
            hlt
    @end2
            %exit =w add 0, 2
            hlt
    }
    $ qsym input.qbe main
    [jnz] Exploring path for label 'end1'
    Halting executing
    Local variables:
    	a = |main:a|
    	exit = #x00000001
    Symbolic variable values:
    	main:a -> #x00000002

    [jnz] Exploring path for label 'end2'
    Halting executing
    Local variables:
    	a = |main:a|
    	exit = #x00000002
    Symbolic variable values:
    	main:a -> #x00000000

For the provided example program, qsym discovers two possible execution paths through the function `main`.
In the first execution path the symbolic variable `%a` is zero, in the other it is non-zero.

### License

This program is free software: you can redistribute it and/or modify it
under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or (at
your option) any later version.

This program is distributed in the hope that it will be useful, but
WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero
General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program. If not, see <https://www.gnu.org/licenses/>.

[qbe web]: https://c9x.me/compile/
[symex wikipedia] https://en.wikipedia.org/wiki/Symbolic_execution
[z3 web]: https://github.com/Z3Prover/z3
[smt wikipedia]: https://en.wikipedia.org/wiki/Satisfiability_modulo_theories
[qbe-reader github]: https://github.com/nmeum/qbe-reader
