use {std::iter::Peekable, Affix::*, Associativity::*};

pub enum Associativity {
    Null,
    Left,
    Right,
}

#[derive(PartialEq, PartialOrd)]
pub struct Precedence(pub u32);

impl Precedence {
    const fn lower(mut self) -> Precedence {
        self.0 -= 1;
        self
    }
    const fn raise(mut self) -> Precedence {
        self.0 += 1;
        self
    }
    const fn min() -> Precedence {
        Precedence(std::u32::MIN)
    }
    const fn max() -> Precedence {
        Precedence(std::u32::MAX)
    }
}

pub enum Arity {
    Nullary,
    Unary,
    Binary,
    Ternary,
}

pub enum Affix {
    Circumfix,
    Interfix,
    Nilfix,
    Prefix,
    Postfix,
    Infix(Associativity),
}

pub struct Op(
    &'static str,
    Affix,
    Arity,
    Precedence,
    &'static [&'static str],
);

impl Op {
    #[inline(always)]
    pub fn new(name: &'static str, affix: Affix, arity: Arity, precedence: Precedence) -> Op {
        Op(name, affix, arity, precedence, &["", "", "", ""])
    }
    #[inline(always)]
    pub const fn followed_by(mut self, names: &'static [&'static str]) -> Op {
        self.4 = names;
        self
    }
}

pub trait PrattParser<Inputs>
where
    Inputs: Iterator<Item = Self::Input>,
{
    type Error: std::fmt::Debug;
    type Input: std::fmt::Debug + std::cmp::PartialEq;
    type Output: Sized + std::fmt::Debug;

    fn query(&mut self, input: &Self::Input) -> Result<Op, Self::Error>;

    fn nullary(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error>;

    fn unary(&mut self, _: Self::Input, _: Self::Output) -> Result<Self::Output, Self::Error> {
        panic!("Encountered unary ouput while it was not implemented");
    }

    fn binary(
        &mut self,
        _: Self::Input,
        _: Self::Output,
        _: Self::Output,
    ) -> Result<Self::Output, Self::Error> {
        panic!("Encountered binary ouput while it was not implemented");
    }

    fn ternary(
        &mut self,
        _: Self::Input,
        _: Self::Output,
        _: Self::Output,
        _: Self::Output,
    ) -> Result<Self::Output, Self::Error> {
        panic!("Encountered ternary ouput while it was not implemented");
    }

    fn parse(&mut self, inputs: &mut Inputs) -> Result<Self::Output, Self::Error> {
        self.parse_until(&mut inputs.peekable(), Precedence::min())
    }

    fn parse_until(
        &mut self,
        inputs: &mut Peekable<&mut Inputs>,
        rbp: Precedence,
    ) -> Result<Self::Output, Self::Error> {
        if let Some(input) = inputs.next() {
            let mut nbp = self.nbp(&input)?;
            let mut node = self.nud(input, inputs);
            loop {
                if let Some(input) = inputs.peek() {
                    let lbp = self.lbp(input)?;
                    if rbp < lbp && lbp < nbp {
                        let input = inputs.next().unwrap();
                        nbp = self.nbp(&input)?;
                        node = self.led(input, inputs, node?);
                    } else {
                        break node;
                    }
                } else {
                    break node;
                }
            }
        } else {
            panic!()
        }
    }

    /// Null-Denotation
    fn nud(
        &mut self,
        input: Self::Input,
        inputs: &mut Peekable<&mut Inputs>,
    ) -> Result<Self::Output, Self::Error> {
        match self.query(&input)? {
            Op(_, Affix::Nilfix, Arity::Nullary, _, _) => self.nullary(input),
            // &a
            Op(_, Affix::Prefix, Arity::Unary, bp, _) => {
                let rbp = bp.lower();
                let rhs = self.parse_until(inputs, rbp)?;
                self.unary(input, rhs)
            }
            // if a then b else c
            Op(_, Affix::Prefix, Arity::Ternary, bp, follow) => {
                let ref mut follow = follow.iter().copied();
                let rbp = bp.lower();
                let lhs = self.parse_until(inputs, Precedence::min())?;
                self.eat_interfix(follow, inputs)?;
                let mid = self.parse_until(inputs, Precedence::min())?;
                self.eat_interfix(follow, inputs)?;
                let rhs = self.parse_until(inputs, rbp)?;
                self.ternary(input, lhs, mid, rhs)
            }
            // ||a||
            Op(_, Affix::Circumfix, Arity::Unary, bp, follow) => {
                let ref mut follow = follow.iter().copied();
                let rbp = bp.lower();
                let rhs = self.parse_until(inputs, rbp)?;
                self.eat_interfix(follow, inputs)?;
                self.unary(input, rhs)
            }
            _ => panic!(
                "Expected unary-prefix or nullary-nilfix operator, found {:?}",
                input
            ),
        }
    }

    fn eat_interfix<F>(
        &mut self,
        follow: &mut F,
        inputs: &mut Peekable<&mut Inputs>,
    ) -> Result<(), Self::Error>
    where
        F: Iterator<Item = &'static str>,
    {
        if let (Some(input), Some(follow)) = (inputs.peek(), follow.next()) {
            if follow != "" {
                match self.query(input)? {
                    Op(name, Affix::Interfix, ..) | Op(name, Affix::Circumfix, ..) => {
                        if name == follow {
                            inputs.next();
                        }
                    }
                    _ => {}
                }
            };
        }

        Ok(())
    }

    /// Left-Denotation
    fn led(
        &mut self,
        input: Self::Input,
        inputs: &mut Peekable<&mut Inputs>,
        lhs: Self::Output,
    ) -> Result<Self::Output, Self::Error> {
        match self.query(&input)? {
            // a!
            Op(_, Affix::Postfix, Arity::Unary, _, _) => self.unary(input, lhs),
            // x = 1 x
            Op(_, Affix::Postfix, Arity::Ternary, bp, follow) => {
                let ref mut follow = follow.iter().copied();
                let rbp = bp.lower();
                let mid = self.parse_until(inputs, Precedence::min())?;
                self.eat_interfix(follow, inputs)?;
                let rhs = self.parse_until(inputs, rbp)?;
                self.ternary(input, lhs, mid, rhs)
            }
            // a + b
            Op(_, Affix::Infix(associativity), Arity::Binary, bp, _) => {
                let rbp = match associativity {
                    Left => bp,
                    Right => bp.lower(),
                    Null => bp,
                };
                let rhs = self.parse_until(inputs, rbp)?;
                self.binary(input, lhs, rhs)
            }
            // a ? b : c
            Op(_, Affix::Infix(associativity), Arity::Ternary, bp, follow) => {
                let ref mut follow = follow.iter().copied();
                let rbp = match associativity {
                    Left => bp,
                    Right => bp.lower(),
                    Null => bp,
                };
                let mid = self.parse_until(inputs, Precedence::min())?;
                self.eat_interfix(follow, inputs)?;
                let rhs = self.parse_until(inputs, rbp)?;
                self.ternary(input, lhs, mid, rhs)
            }
            _ => panic!(
                "Expected unary-postfix or binary-infix expression, found {:?}",
                input
            ),
        }
    }

    /// Left-Binding-Power
    fn lbp(&mut self, input: &Self::Input) -> Result<Precedence, Self::Error> {
        let lbp = match self.query(input)? {
            Op(_, Interfix, ..) => Precedence::min(),
            Op(_, Circumfix, ..) => Precedence::min(),
            Op(_, Nilfix, ..) => Precedence::min(),
            Op(_, Prefix, ..) => Precedence::min(),
            Op(_, Postfix, _, bp, _) => bp,
            Op(_, Infix(_), _, bp, _) => bp,
        };
        Ok(lbp)
    }

    //         <lbp>  <rbp>  <nbp> <kind>
    // Nilfix:  MIN |  MIN |  MAX | nud
    // Prefix:  MIN |   bp |  MAX | nud
    // Postfix:  bp |  MIN |  MAX | led
    // InfixL:   bp |   bp | bp+1 | led
    // InfixR:   bp | bp-1 | bp+1 | led
    // InfixN:   bp |   bp |   bp | led
    // Mixfix:

    /// Next-Binding-Power
    fn nbp(&mut self, input: &Self::Input) -> Result<Precedence, Self::Error> {
        let nbp = match self.query(input)? {
            Op(_, Interfix, ..) => Precedence::max(),
            Op(_, Circumfix, ..) => Precedence::max(),
            Op(_, Nilfix, ..) => Precedence::max(),
            Op(_, Prefix, ..) => Precedence::max(),
            Op(_, Postfix, ..) => Precedence::max(),
            Op(_, Infix(Left), _, bp, _) => bp.raise(),
            Op(_, Infix(Right), _, bp, _) => bp.raise(),
            Op(_, Infix(Null), _, bp, _) => bp,
        };
        Ok(nbp)
    }
}
