use std::path::{Path, PathBuf};

use clap::Parser;
use rand::prelude::*;
use rust_htslib::bcf::header::Header;
use rust_htslib::bcf::record::GenotypeAllele;
use rust_htslib::bcf::Read;
use rust_htslib::bcf::{Format, Reader, Writer};

pub fn make_test_vcf(file: &str) {
    // Create minimal VCF header with a single contig and a single sample
    let mut header = Header::new();

    // header lines
    let contig_str = r#"##contig=<ID=1,length=1000>"#;
    let format_gt_str = r#"##FORMAT=<ID=GT,Number=1,Type=String,Description="Genotype">"#;
    let format_ad_str = r#"##FORMAT=<ID=AD,Number=1,Type=Integer,Description="Genotype">"#;
    header.push_record(contig_str.as_bytes());
    header.push_record(format_gt_str.as_bytes());
    header.push_record(format_ad_str.as_bytes());

    // header samples
    let mut samples = Vec::<String>::new();
    for i in 0..10 {
        samples.push(format!("Sample{}", i));
    }
    for sample in samples.iter() {
        header.push_sample(sample.as_bytes());
    }

    // Write uncompressed VCF to stdout with above header and get an empty record
    let mut vcf = Writer::from_path(file, &header, true, Format::Vcf).unwrap();
    let headerview = vcf.header();
    let rid = headerview.name2rid(b"1").unwrap();

    let mut record = vcf.empty_record();

    let mut alleles = Vec::<GenotypeAllele>::with_capacity(2 * 10);
    let mut ads = Vec::<i32>::with_capacity(2 * 10);

    let mut rng = rand::thread_rng();

    for pos in 0..1000 {
        if pos % 10 != 0 {
            continue;
        }

        // Set chrom and pos to 1 and 7, respectively - note the 0-based positions
        record.set_rid(Some(rid));
        record.set_pos(6);

        // Set record genotype to 0|1 - note first allele is always unphased
        for _ in 0..10 {
            let mut allele: i32 = 0;
            if rng.gen::<f64>() > 0.90f64 {
                allele = 1;
            }
            alleles.push(GenotypeAllele::Unphased(allele));
            allele = 0;
            if rng.gen::<f64>() > 0.90f64 {
                allele = 1;
            }
            alleles.push(GenotypeAllele::Unphased(allele));
        }
        record.push_genotypes(&alleles).unwrap();

        for _ in 0..10 {
            let mut ad: u32 = rng.gen::<u32>() % 100;
            ads.push(ad as i32);
            ad = rng.gen::<u32>() % 100;
            ads.push(ad as i32);
        }
        let tag = "AD".as_bytes();
        record.push_format_integer(tag, &ads).unwrap();

        vcf.write(&record).unwrap();

        record.clear();
        alleles.clear();
        ads.clear();
    }

    // Write record
}

fn mask_vcf(in_file: &Path, out_file: &Path, min_ad: i32, uncompressed: bool, vcf_format: bool) {
    let mut in_vcf = Reader::from_path(in_file).unwrap();
    let in_header = in_vcf.header().to_owned();
    let out_header = Header::from_template(&in_header);
    let mut out_alleles = Vec::<GenotypeAllele>::new();

    let vcf_format = if vcf_format { Format::Vcf } else { Format::Bcf };
    let mut out_vcf = Writer::from_path(out_file, &out_header, uncompressed, vcf_format).unwrap();

    let mut ploidy_prev = 0usize;
    // get record from input vcf
    for record_res in in_vcf.records() {
        let mut record = record_res.unwrap();
        // get ads
        let in_ads = record.format(b"AD").integer().unwrap();
        // get genotypes
        let gts = record.genotypes().unwrap();
        for i in 0..in_header.sample_count() {
            // get sample-specific slices
            let alleles = gts.get(i as usize);
            let ads = in_ads[i as usize];

            // get total allele depth per genotype call
            let total_ads: i32 = ads.iter().sum();
            // get ploidy
            let ploidy = alleles.len();

            if i == 0 {
                ploidy_prev = ploidy;
            } else {
                assert_eq!(ploidy_prev, ploidy);
                ploidy_prev = ploidy;
            }

            // when AD support is enough for genotype call
            if total_ads >= min_ad {
                out_alleles.extend(alleles.iter());
            }
            // otherwise, set genotype allele to missing
            else {
                for _ in 0..ploidy {
                    out_alleles.push(GenotypeAllele::UnphasedMissing);
                }
            }
        }

        // modifiy the record read from input vcf
        record.push_genotypes(&out_alleles).unwrap();

        // write modified record
        out_vcf.write(&record).unwrap();
        out_alleles.clear();
    }
}

#[test]
fn test_() {
    make_test_vcf("1.vcf");
    mask_vcf(&Path::new("1.vcf"), &Path::new("2.vcf"), 50, false, true);
}

#[derive(Parser, Debug)]
#[command(author = "Bing Guo<gbinux@gmail.com>")]
#[command(version)]
#[command(
    about = "Mask genotypes with low AD",
    long_about = "Mask genotype calls for those with low sequence depth support"
)]
struct Cli {
    /// Input VCF file path
    #[arg(short, long)]
    invcf: PathBuf,

    /// Output VCF file
    #[arg(short, long)]
    outvcf: PathBuf,

    /// Output file will be un_compressed
    #[arg(short, long, default_value_t = false)]
    uncompress: bool,

    /// Output file is vcf format. False meams bcf
    #[arg(short, long, default_value_t = false)]
    vcf_format: bool,

    /// minimum total allele depth for sample
    #[arg(short, long, default_value_t = 5)]
    min_depth: i32,
}

fn main() {
    let args = Cli::parse();
    mask_vcf(
        &args.invcf,
        &args.outvcf,
        args.min_depth,
        args.uncompress,
        args.vcf_format,
    )
}
