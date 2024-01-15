use handlebars::{
    to_json, BlockContext, Context, Handlebars, Helper, HelperDef, HelperResult, JsonValue, Output,
    PathAndJson, RenderContext, RenderError, RenderErrorReason, Renderable,
};

pub(crate) fn create_block<'reg: 'rc, 'rc>(param: &'rc PathAndJson<'rc>) -> BlockContext<'reg> {
    let mut block = BlockContext::new();

    if let Some(new_path) = param.context_path() {
        *block.base_path_mut() = new_path.clone();
    } else {
        // use clone for now
        block.set_base_value(param.value().clone());
    }

    block
}

#[derive(Clone, Copy)]
pub struct ForRangHelper;

impl HelperDef for ForRangHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'rc>,
        r: &'reg Handlebars<'reg>,
        ctx: &'rc Context,
        rc: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> HelperResult {
        let value = h
            .param(0)
            .ok_or_else(|| RenderErrorReason::ParamNotFoundForIndex("forRange", 0))?;

        let template = h.template();
        match template {
            Some(t) => match *value.value() {
                JsonValue::Number(ref number) => {
                    let block_context = create_block(value);
                    rc.push_block(block_context);

                    let number = number
                        .as_u64()
                        .ok_or_else(|| RenderErrorReason::Other("bad u64 conversion".into()))?;
                    for i in 0..number {
                        if let Some(ref mut block) = rc.block_mut() {
                            let is_first = i == 0u64;
                            let is_last = i == number - 1;

                            let index = to_json(i);
                            block.set_local_var("first", to_json(is_first));
                            block.set_local_var("last", to_json(is_last));
                            block.set_local_var("index", index);
                        }

                        t.render(r, ctx, rc, out)?;
                    }

                    rc.pop_block();
                    Ok(())
                }
                _ => {
                    if r.strict_mode() {
                        Err(RenderError::strict_error(value.relative_path()))
                    } else {
                        Ok(())
                    }
                }
            },
            None => Ok(()),
        }
    }
}
