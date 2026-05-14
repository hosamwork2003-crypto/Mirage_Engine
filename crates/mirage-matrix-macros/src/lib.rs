use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, DeriveInput, Data, Fields};

#[proc_macro_derive(NeuralCluster, attributes(synapse))]
pub fn neural_cluster_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let mut field_names = Vec::new();
    let mut field_types = Vec::new(); 
    let mut setter_names = Vec::new(); 
    
    if let Data::Struct(data) = input.data {
        if let Fields::Named(fields) = data.fields {
            for field in fields.named {
                if let Some(ident) = &field.ident {
                    field_names.push(ident.clone());
                    field_types.push(field.ty.clone());
                    setter_names.push(format_ident!("set_{}", ident));
                }
            }
        }
    }

    let expanded = quote! {
        impl #name {
            pub fn wire_to_matrix(&self, matrix: &mut NeuralMatrix) -> Vec<petgraph::graph::NodeIndex> {
                println!("🔌 Wiring Cluster [{}] into the Neural Matrix...", stringify!(#name));
                let mut indices = std::vec::Vec::new();
                
                #(
                    let node_id = uuid::Uuid::new_v4();
                    let idx = matrix.graph.add_node(node_id);
                    indices.push(idx);
                    
                    // 💡 وجود #field_names هنا هو اللي بيحل الإيرور وبيخلي الماكرو يلف صح
                    println!("   🟢 Wired Node [{}]: {}", stringify!(#field_names), node_id);
                )*
                
                indices 
            }

            #(
                pub fn #setter_names(
                    &mut self, 
                    new_value: #field_types, 
                    matrix: &NeuralMatrix, 
                    node_idx: petgraph::graph::NodeIndex
                ) {
                    self.#field_names = new_value;
                    println!("💉 Mutation on [{}]: Value changed. Injecting pulse...", stringify!(#field_names));
                    matrix.trace_impact(node_idx);
                }
            )*
        }
    };

    TokenStream::from(expanded)
}