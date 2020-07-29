#![allow(unused_imports)]

use anyhow::{bail, Result};
use byte_slice_cast::IntoByteVec;
use relative_path::{RelativePath, RelativePathBuf};
use shader_prepper;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use turbosloth::*;

#[derive(Clone, Hash)]
pub struct CompileComputeShader {
    pub path: PathBuf,
}

#[async_trait]
impl LazyWorker for CompileComputeShader {
    type Output = Result<ComputeShader>;

    async fn run(self, ctx: RunContext) -> Self::Output {
        let file_path = self.path.to_str().unwrap().to_owned();
        let source = shader_prepper::process_file(
            &file_path,
            &mut ShaderIncludeProvider { ctx: ctx.clone() },
            String::new(),
        );
        let source = source.map_err(|err| anyhow::anyhow!("{}", err))?;

        let ext = self
            .path
            .extension()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or("".to_string());

        let name = self
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or("unknown".to_string());

        match ext.as_str() {
            "glsl" => unimplemented!(),
            "hlsl" => compile_cs_hlsl_impl(name, &source),
            _ => anyhow::bail!("Unrecognized shader file extension: {}", ext),
        }
    }
}

pub struct ComputeShader {
    pub name: String,
    pub group_size: [u32; 3],
    pub spirv: Vec<u8>,
    pub srvs: Vec<String>,
    pub uavs: Vec<String>,
}

fn compile_cs_hlsl_impl(
    name: String,
    source: &[shader_prepper::SourceChunk],
) -> Result<ComputeShader> {
    let refl = {
        //let spirv = shaderc_compile_glsl(&name, source, shaderc::ShaderKind::Compute)?;
        let mut source_text = String::new();
        for s in source {
            source_text += &s.source;
        }

        let t0 = std::time::Instant::now();
        let spirv =
            hassle_rs::compile_hlsl(&name, &source_text, "main", "cs_6_4", &["-spirv"], &[])
                .map_err(|err| anyhow::anyhow!("{}", err))?;
        println!("dxc took {:?}", t0.elapsed());

        use byte_slice_cast::*;
        reflect_spirv_shader(spirv.as_slice_of::<u32>()?)?
    };

    let spirv = refl.get_code();
    let local_size = get_cs_local_size_from_spirv(&spirv)?;

    let mut srvs = Vec::new();
    let mut uavs = Vec::new();

    let descriptor_sets = refl.enumerate_descriptor_sets(None).unwrap();
    {
        let set = &descriptor_sets[0];
        for binding_index in 0..set.bindings.len() {
            let binding = &set.bindings[binding_index];
            assert_ne!(
                binding.resource_type,
                spirv_reflect::types::resource::ReflectResourceType::Undefined
            );
            match binding.resource_type {
                spirv_reflect::types::resource::ReflectResourceType::ShaderResourceView => {
                    srvs.push(binding.name.clone());
                }
                spirv_reflect::types::resource::ReflectResourceType::UnorderedAccessView => {
                    uavs.push(binding.name.clone());
                }
                _ => {}
            };
        }
    }

    Ok(ComputeShader {
        name,
        group_size: local_size,
        spirv: spirv.into_byte_vec(),
        srvs,
        uavs,
    })
}

struct ShaderIncludeProvider {
    ctx: RunContext,
}

impl<'a> shader_prepper::IncludeProvider for ShaderIncludeProvider {
    type IncludeContext = String;

    fn get_include(
        &mut self,
        path: &str,
        parent_file: &Self::IncludeContext,
    ) -> std::result::Result<(String, Self::IncludeContext), failure::Error> {
        let path = if let Some('/') = path.chars().next() {
            path.chars().skip(1).collect()
        } else {
            let mut folder: RelativePathBuf = parent_file.into();
            folder.pop();
            folder.join(path).as_str().to_string()
        };

        let blob = smol::block_on(
            crate::file::LoadFile {
                path: PathBuf::from(&path),
            }
            .into_lazy()
            .eval(&self.ctx),
        )
        .map_err(|err| failure::format_err!("{}", err))?;

        String::from_utf8((*blob).clone())
            .map_err(|e| failure::format_err!("{}", e))
            .map(|ok| (ok, path))
    }
}

fn reflect_spirv_shader(shader_code: &[u32]) -> Result<spirv_reflect::ShaderModule> {
    //println!("+reflect_spirv_shader");
    let res = convert_spirv_reflect_err(spirv_reflect::ShaderModule::load_u32_data(shader_code));
    //println!("-reflect_spirv_shader");
    res
}

fn get_cs_local_size_from_spirv(spirv: &[u32]) -> Result<[u32; 3]> {
    let mut loader = rspirv::dr::Loader::new();
    rspirv::binary::parse_words(spirv, &mut loader).unwrap();
    let module = loader.module();

    for inst in module.global_inst_iter() {
        //if spirv_headers::Op::ExecutionMode == inst.class.opcode {
        if inst.class.opcode as u32 == 16 {
            let local_size = &inst.operands[2..5];
            use rspirv::dr::Operand::LiteralInt32;

            if let &[LiteralInt32(x), LiteralInt32(y), LiteralInt32(z)] = local_size {
                return Ok([x, y, z]);
            } else {
                bail!("Could not parse the ExecutionMode SPIR-V op");
            }
        }
    }

    bail!("Could not find a ExecutionMode SPIR-V op");
}

fn convert_spirv_reflect_err<T>(res: std::result::Result<T, &'static str>) -> Result<T> {
    match res {
        Ok(res) => Ok(res),
        Err(e) => bail!("SPIR-V reflection error: {}", e),
    }
}
